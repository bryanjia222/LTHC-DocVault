use std::{path::Path, sync::LazyLock, time::Duration};

use http_body::{Body as HttpBody, Frame, SizeHint};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_rustls::HttpsConnector;
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::{TokioExecutor, TokioTimer},
};
use serde::de::DeserializeOwned;

use super::types::RawEnvelope;

pub(super) const UPLOAD_BOUNDARY: &str = "DocVaultQinbixinMediaUpload";

fn format_error_chain<E: std::error::Error>(error: E) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        message.push_str(": ");
        message.push_str(&error.to_string());
        source = error.source();
    }
    message
}

type QinbixinBody = http_body_util::combinators::BoxBody<Bytes, std::convert::Infallible>;
type QinbixinHttpClient = Client<HttpsConnector<HttpConnector>, QinbixinBody>;

static HTTP_CLIENT: LazyLock<QinbixinHttpClient> = LazyLock::new(|| {
    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .build();
    Client::builder(TokioExecutor::new())
        .timer(TokioTimer::new())
        .http1_max_buf_size(1_000_000)
        .http1_max_headers(1_000)
        .build(https)
});

pub(super) async fn request_json<T: DeserializeOwned>(
    base_url: &str,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<(RawEnvelope<T>, Option<String>), String> {
    let uri = format!("{base_url}{path}")
        .parse::<hyper::Uri>()
        .map_err(|e| crate::logging::log_error(format!("invalid request URL: {e}")))?;
    let request = hyper::Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("accept", "*/*")
        .header("user-agent", "DocVault/0.2");
    let request = if let Some(body) = body {
        let text = serde_json::to_vec(&body)
            .map_err(|e| crate::logging::log_error(format!("unable to encode request: {e}")))?;
        request.header("content-type", "application/json").body(
            http_body_util::combinators::BoxBody::new(Full::new(Bytes::from(text))),
        )
    } else {
        request.body(http_body_util::combinators::BoxBody::new(Full::new(
            Bytes::new(),
        )))
    }
    .map_err(|e| crate::logging::log_error(format!("unable to create request: {e}")))?;
    let response = tokio::time::timeout(Duration::from_secs(10), HTTP_CLIENT.request(request))
        .await
        .map_err(|_| crate::logging::log_error("request timed out"))?
        .map_err(|e| {
            crate::logging::log_error(format!("request failed: {}", format_error_chain(e)))
        })?;
    let new_token = response
        .headers()
        .get("NewToken")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let status = response.status();
    let bytes = response.into_body().collect().await.map_err(|e| {
        crate::logging::log_error(format!(
            "unable to read response: {}",
            format_error_chain(e)
        ))
    })?;
    let text = String::from_utf8_lossy(bytes.to_bytes().as_ref()).into_owned();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(crate::logging::log_warn("AUTH_EXPIRED"));
    }
    serde_json::from_str(&text)
        .map(|envelope| (envelope, new_token))
        .map_err(|e| crate::logging::log_error(format!("invalid response: {e}; body: {text}")))
}

pub(super) fn mapped_error<T>(envelope: RawEnvelope<T>) -> Result<T, String> {
    if envelope.success.unwrap_or_default() {
        envelope
            .data
            .ok_or_else(|| crate::logging::log_error("missing response data"))
    } else {
        let message = envelope
            .msg
            .unwrap_or_else(|| crate::logging::log_error("request failed"));
        tracing::warn!(message = %message, "qinbixin request rejected");
        Err(message)
    }
}

pub(super) fn mime_for_file(file_name: &str) -> &'static str {
    match Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "tiff" | "tif" => "image/tiff",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "ogg" => "video/ogg",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

fn multipart_file_body(file_name: &str, content_type: &str, bytes: Vec<u8>) -> Bytes {
    let escaped_name = file_name.replace('\\', "\\\\").replace('"', "\\\"");
    let mut body = Vec::with_capacity(bytes.len() + 512);
    body.extend_from_slice(
        format!(
            "--{UPLOAD_BOUNDARY}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"{escaped_name}\"\r\n\
             Content-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(&bytes);
    body.extend_from_slice(format!("\r\n--{UPLOAD_BOUNDARY}--\r\n").as_bytes());
    Bytes::from(body)
}

struct ProgressBody {
    data: Bytes,
    offset: usize,
    chunk_size: usize,
    total: usize,
    on_progress: Box<dyn Fn(usize, usize) + Send + Sync>,
}

impl ProgressBody {
    fn new(data: Bytes, on_progress: Box<dyn Fn(usize, usize) + Send + Sync>) -> Self {
        let total = data.len();
        Self {
            data,
            offset: 0,
            chunk_size: 64 * 1024,
            total,
            on_progress,
        }
    }
}

impl HttpBody for ProgressBody {
    type Data = Bytes;
    type Error = std::convert::Infallible;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        if self.offset >= self.total {
            return std::task::Poll::Ready(None);
        }
        let end = (self.offset + self.chunk_size).min(self.total);
        let chunk = self.data.slice(self.offset..end);
        self.offset = end;
        (self.on_progress)(self.offset, self.total);
        std::task::Poll::Ready(Some(Ok(Frame::data(chunk))))
    }

    fn is_end_stream(&self) -> bool {
        self.offset >= self.total
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.total as u64)
    }
}

pub(super) async fn request_multipart<T: DeserializeOwned>(
    base_url: &str,
    token: &str,
    path: &str,
    file_name: &str,
    bytes: Vec<u8>,
    on_progress: Option<Box<dyn Fn(usize, usize) + Send + Sync>>,
) -> Result<(RawEnvelope<T>, Option<String>), String> {
    let uri = format!("{base_url}{path}")
        .parse::<hyper::Uri>()
        .map_err(|e| crate::logging::log_error(format!("invalid upload URL: {e}")))?;
    let content_type = format!("multipart/form-data; boundary={UPLOAD_BOUNDARY}");
    let body = multipart_file_body(file_name, mime_for_file(file_name), bytes);
    let request = hyper::Request::builder()
        .method(reqwest::Method::POST)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("accept", "*/*")
        .header("user-agent", "DocVault/0.2")
        .header("content-type", content_type)
        .header("content-length", body.len())
        .body(match on_progress {
            Some(cb) => http_body_util::combinators::BoxBody::new(ProgressBody::new(body, cb)),
            None => http_body_util::combinators::BoxBody::new(Full::new(body)),
        })
        .map_err(|e| crate::logging::log_error(format!("unable to create upload request: {e}")))?;
    let response = tokio::time::timeout(Duration::from_secs(120), HTTP_CLIENT.request(request))
        .await
        .map_err(|_| crate::logging::log_error("upload timed out"))?
        .map_err(|e| {
            crate::logging::log_error(format!("upload failed: {}", format_error_chain(e)))
        })?;
    let new_token = response
        .headers()
        .get("NewToken")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let status = response.status();
    let response_bytes = response.into_body().collect().await.map_err(|e| {
        crate::logging::log_error(format!(
            "unable to read upload response: {}",
            format_error_chain(e)
        ))
    })?;
    let text = String::from_utf8_lossy(response_bytes.to_bytes().as_ref()).into_owned();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(crate::logging::log_warn("AUTH_EXPIRED"));
    }
    serde_json::from_str(&text)
        .map(|envelope| (envelope, new_token))
        .map_err(|e| {
            crate::logging::log_error(format!("invalid upload response: {e}; body: {text}"))
        })
}

pub(super) fn absolute_url(base_url: &str, url: String) -> String {
    if url.is_empty() || url.starts_with("http://") || url.starts_with("https://") {
        return url;
    }
    if url.starts_with("//") {
        let scheme = if base_url.starts_with("https://") {
            "https"
        } else {
            "http"
        };
        return format!("{scheme}:{url}");
    }
    if url.starts_with('/') {
        return format!("{base_url}{url}");
    }
    format!("{base_url}/{url}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_multipart_file_body() {
        let body = multipart_file_body("letter image.png", "image/png", vec![1, 2, 3]);
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.starts_with(&format!("--{UPLOAD_BOUNDARY}\r\n")));
        assert!(text.contains(r#"name="file"; filename="letter image.png""#));
        assert!(text.contains("Content-Type: image/png"));
        assert!(text.ends_with(&format!("--{UPLOAD_BOUNDARY}--\r\n")));
    }

    #[test]
    fn resolves_relative_asset_urls() {
        let base = "https://qinbixin.com.cn";
        assert_eq!(
            absolute_url(base, "/uploadfiles/avatar.jpg".to_owned()),
            "https://qinbixin.com.cn/uploadfiles/avatar.jpg"
        );
        assert_eq!(
            absolute_url(base, "uploadfiles/avatar.jpg".to_owned()),
            "https://qinbixin.com.cn/uploadfiles/avatar.jpg"
        );
        assert_eq!(
            absolute_url(base, "https://qinbixin.com.cn/avatar.jpg".to_owned()),
            "https://qinbixin.com.cn/avatar.jpg"
        );
    }
}
