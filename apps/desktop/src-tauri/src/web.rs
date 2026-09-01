//! Web helpers for the sidebar "quick links" feature: opening a URL in the
//! system default browser and best-effort fetching a page's `<title>` + favicon
//! for the add-link flow. Every network failure degrades to `None` - the fetch
//! command never fails, the frontend falls back to a generic icon + the raw URL.

use base64::Engine;
use serde::Serialize;

/// Best-effort metadata about a web page, fetched for the add-link flow.
/// `favicon` is a `data:` URL (already base64) so it renders inside the webview
/// without widening the CSP's `img-src` to arbitrary https hosts.
#[derive(Debug, Default, Serialize)]
pub struct UrlMeta {
    pub title: Option<String>,
    pub favicon: Option<String>,
}

const PAGE_BODY_LIMIT: usize = 2_000_000;
const ICON_BODY_LIMIT: usize = 1_000_000;
const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) DocVault/0.1";

/// Fetch `<title>` + favicon for `url`. Never fails: every error path returns
/// `UrlMeta { title: None, favicon: None }` so the link can always be added.
#[tauri::command]
pub async fn fetch_url_meta(url: String) -> Result<UrlMeta, String> {
    Ok(fetch_meta(&url).await)
}

/// Open `url` in the system default browser (http/https only). Reuses the
/// already-vendored `open` crate - no shell plugin / capability needed.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    if !is_openable_url(&url) {
        return Err(crate::logging::log_warn(
            "only http/https URLs can be opened",
        ));
    }
    open::that(&url).map_err(crate::logging::log_error)
}

fn is_openable_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Upper bound on meta-refresh redirect hops. Some sites (e.g. baidu) serve a
/// tiny `<noscript><meta http-equiv="refresh">` stub that downgrades https ->
/// http; the fetcher does not run JS, so it follows the refresh itself.
const MAX_META_REFRESH_HOPS: usize = 3;

async fn fetch_meta(url: &str) -> UrlMeta {
    if !is_openable_url(url) {
        tracing::warn!(url, "quick link metadata unavailable: unsupported URL");
        return UrlMeta::default();
    }
    let Ok(client) = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
    else {
        tracing::warn!(url, "quick link metadata unavailable: HTTP client failed");
        return UrlMeta::default();
    };
    let mut current = url.to_string();
    let mut visited = std::collections::HashSet::new();
    for _ in 0..MAX_META_REFRESH_HOPS {
        if !visited.insert(current.clone()) {
            tracing::warn!(url, "quick link metadata unavailable: refresh loop");
            return UrlMeta::default();
        }
        let Ok(resp) = client.get(&current).send().await else {
            tracing::warn!(url, current_url = %current, "quick link metadata unavailable: fetch failed");
            return UrlMeta::default();
        };
        if resp
            .content_length()
            .map(|l| l as usize > PAGE_BODY_LIMIT)
            .unwrap_or(false)
        {
            tracing::warn!(url, current_url = %current, "quick link metadata unavailable: page too large");
            return UrlMeta::default();
        }
        let final_url = resp.url().clone();
        let Ok(bytes) = resp.bytes().await else {
            tracing::warn!(url, current_url = %current, "quick link metadata unavailable: body read failed");
            return UrlMeta::default();
        };
        let bytes = &bytes[..bytes.len().min(PAGE_BODY_LIMIT)];
        let html = decode_html(bytes);
        let (title, icon_href) = extract_page_meta(&html, final_url.as_str());
        // A real <title> is authoritative - stop (a refresh would be redundant).
        if title.is_some() {
            let favicon = match icon_href {
                Some(href) => fetch_favicon(&client, &href).await,
                None => None,
            };
            return UrlMeta { title, favicon };
        }
        // Otherwise follow a meta-refresh (e.g. baidu's https -> http stub).
        if let Some(next) = extract_refresh_url(&html, final_url.as_str()) {
            current = next;
            continue;
        }
        return UrlMeta {
            title: None,
            favicon: None,
        };
    }
    tracing::warn!(
        url,
        "quick link metadata unavailable: refresh hop limit reached"
    );
    UrlMeta::default()
}

/// Extract the target of the first `<meta http-equiv="refresh" content="N;url=...">`.
fn extract_refresh_url(html: &str, base_url: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut search_from = 0usize;
    while search_from < lower.len() {
        let lt = lower[search_from..].find("<meta")? + search_from;
        let gt = lower[lt..].find('>')? + lt;
        let tag = &html[lt..gt];
        if let Some(equiv) = attr_value(tag, "http-equiv") {
            if equiv.to_ascii_lowercase().trim() == "refresh" {
                if let Some(content) = attr_value(tag, "content") {
                    if let Some(target) = extract_url_from_refresh_content(&content) {
                        return resolve_url(base_url, &target);
                    }
                }
            }
        }
        search_from = gt + 1;
    }
    None
}

fn extract_url_from_refresh_content(content: &str) -> Option<String> {
    let lower = content.to_ascii_lowercase();
    let pos = lower.find("url=")?;
    let rest = content[pos + 4..]
        .trim()
        .trim_matches(|c| c == '"' || c == '\'' || c == ';' || c == ' ');
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

async fn fetch_favicon(client: &reqwest::Client, href: &str) -> Option<String> {
    let Ok(resp) = client.get(href).send().await else {
        tracing::warn!(url = %href, "quick link favicon unavailable: fetch failed");
        return None;
    };
    if resp
        .content_length()
        .map(|l| l as usize > ICON_BODY_LIMIT)
        .unwrap_or(false)
    {
        tracing::warn!(url = %href, "quick link favicon unavailable: too large");
        return None;
    }
    let declared = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| {
            s.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
        })
        .unwrap_or_default();
    let Ok(bytes) = resp.bytes().await else {
        tracing::warn!(url = %href, "quick link favicon unavailable: body read failed");
        return None;
    };
    let bytes = &bytes[..bytes.len().min(ICON_BODY_LIMIT)];
    if bytes.is_empty() {
        tracing::warn!(url = %href, "quick link favicon unavailable: empty response");
        return None;
    }
    let mime = sniff_mime(bytes)
        .map(str::to_owned)
        .or_else(|| declared.starts_with("image/").then_some(declared))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{mime};base64,{b64}"))
}

/// Sniff a common image format from magic bytes - favicon bytes can arrive with
/// no Content-Type or a bogus one (e.g. text/html for an image URL).
fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else if bytes.starts_with(b"<svg") {
        Some("image/svg+xml")
    } else if bytes.len() >= 4 && bytes[0] == 0 && bytes[1] == 0 && bytes[2] == 1 && bytes[3] == 0 {
        Some("image/x-icon")
    } else {
        None
    }
}

/// Decode a page body honoring a declared legacy charset (GBK/GB2312/Big5), so
/// old Chinese pages still yield readable titles. Everything else (incl. UTF-8,
/// the modern default) goes through UTF-8 lossy - a wrong declaration can never
/// panic.
fn decode_html(bytes: &[u8]) -> String {
    let head = &bytes[..bytes.len().min(1024)];
    // Strip quotes so `<meta charset="gbk">` and `charset=gbk` both match.
    let head_norm = String::from_utf8_lossy(head)
        .to_ascii_lowercase()
        .replace(['"', '\''], "");
    let encoding = if head_norm.contains("charset=gbk") || head_norm.contains("charset=gb2312") {
        Some(encoding_rs::GBK)
    } else if head_norm.contains("charset=big5") {
        Some(encoding_rs::BIG5)
    } else {
        None
    };
    match encoding {
        Some(enc) => enc.decode(bytes).0.into_owned(),
        None => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Extract the page `<title>` and the first `<link rel="...icon...">` href from
/// HTML, returning `(title, icon_href)`. `icon_href` is resolved against
/// `base_url` (the response's final URL after redirects) so relative favicon
/// paths become absolute. Best-effort; `None` on any miss.
pub fn extract_page_meta(html: &str, base_url: &str) -> (Option<String>, Option<String>) {
    let title = extract_title(html);
    let icon_href = extract_icon_href(html).and_then(|href| resolve_url(base_url, &href));
    (title, icon_href)
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let gt = lower[start..].find('>')? + start + 1;
    let close = lower[gt..].find("</title>")? + gt;
    let title = html[gt..close].trim();
    if title.is_empty() {
        return None;
    }
    Some(title.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn extract_icon_href(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut search_from = 0usize;
    while search_from < lower.len() {
        let lt = lower[search_from..].find("<link")? + search_from;
        let gt = lower[lt..].find('>')? + lt;
        let tag = &html[lt..gt];
        if let Some(rel) = attr_value(tag, "rel") {
            if rel.to_ascii_lowercase().contains("icon") {
                if let Some(href) = attr_value(tag, "href") {
                    return Some(href);
                }
            }
        }
        search_from = gt + 1;
    }
    None
}

/// Read an attribute value out of a single `<tag ...>` slice, matching the name
/// case-insensitively, without crossing the tag boundary. Handles quoted and
/// unquoted values. `None` when absent or empty.
fn attr_value(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let needle = format!("{name}=");
    let mut idx = 0usize;
    while idx < lower.len() {
        let pos = lower[idx..].find(&needle)?;
        let eq = idx + pos + needle.len();
        let after = &lower[eq..];
        let value = if after.starts_with('"') || after.starts_with('\'') {
            let quote = after.chars().next().unwrap();
            let vs = eq + 1;
            let ve = lower[vs..].find(quote)? + vs;
            if lower[vs..ve].contains('<') {
                return None;
            }
            &tag[vs..ve]
        } else {
            let end = after.find([' ', '>']).unwrap_or(after.len());
            let ve = eq + end;
            if lower[eq..ve].contains('<') {
                return None;
            }
            &tag[eq..ve]
        };
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
        idx = eq;
    }
    None
}

fn resolve_url(base: &str, href: &str) -> Option<String> {
    let base = reqwest::Url::parse(base).ok()?;
    base.join(href).ok().map(|u| u.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_is_extracted_and_whitespace_collapsed() {
        let html = "<html><head>\n  <title>  My\n Page </title></head><body></body></html>";
        assert_eq!(extract_title(html).as_deref(), Some("My Page"));
    }

    #[test]
    fn title_missing_returns_none() {
        assert_eq!(extract_title("<html><body>hi</body></html>"), None);
    }

    #[test]
    fn icon_href_is_found_and_resolved_relative() {
        let html = r#"<link rel="stylesheet" href="/x.css"><link rel="icon" type="image/png" href="/favicon.png">"#;
        let (_, icon) = extract_page_meta(html, "https://example.com/a/page");
        assert_eq!(icon.as_deref(), Some("https://example.com/favicon.png"));
    }

    #[test]
    fn icon_href_absolute_stays_absolute() {
        let html = r#"<link rel="shortcut icon" href="https://cdn.example.com/fav.ico">"#;
        let (_, icon) = extract_page_meta(html, "https://example.com/");
        assert_eq!(icon.as_deref(), Some("https://cdn.example.com/fav.ico"));
    }

    #[test]
    fn unquoted_icon_href_works() {
        let html = r#"<link rel=icon href=/favicon.png>"#;
        assert_eq!(extract_icon_href(html).as_deref(), Some("/favicon.png"));
    }

    #[test]
    fn no_icon_returns_none() {
        assert_eq!(
            extract_icon_href(r#"<link rel="stylesheet" href="/x.css">"#),
            None
        );
    }

    #[test]
    fn gbk_title_decodes() {
        let html =
            "<html><head><meta charset=\"gbk\"><title>中文标题</title></head><body></body></html>";
        let bytes = encoding_rs::GBK.encode(html).0;
        let text = decode_html(&bytes);
        assert_eq!(extract_title(&text).as_deref(), Some("中文标题"));
    }

    #[test]
    fn sniff_recognizes_png_ico_and_rejects_text() {
        assert_eq!(sniff_mime(b"\x89PNG\r\n\x1a\n\x00"), Some("image/png"));
        assert_eq!(
            sniff_mime(b"\x00\x00\x01\x00\x00\x00\x00\x00"),
            Some("image/x-icon")
        );
        assert_eq!(sniff_mime(b"<html>not an image"), None);
    }

    #[test]
    fn open_url_scheme_validation() {
        assert!(!is_openable_url("file:///etc/passwd"));
        assert!(!is_openable_url("javascript:alert(1)"));
        assert!(is_openable_url("https://example.com"));
        assert!(is_openable_url("http://example.com"));
    }

    #[test]
    fn meta_refresh_url_extracted_and_resolved() {
        let html = r#"<html><head><noscript><meta http-equiv="refresh" content="0;url=http://www.baidu.com/"></noscript></head></html>"#;
        assert_eq!(
            extract_refresh_url(html, "https://www.baidu.com").as_deref(),
            Some("http://www.baidu.com/")
        );
    }

    #[test]
    fn meta_refresh_relative_url_resolved() {
        let html = r#"<meta http-equiv="refresh" content="2; URL=/next">"#;
        assert_eq!(
            extract_refresh_url(html, "https://example.com/a/b").as_deref(),
            Some("https://example.com/next")
        );
    }

    #[test]
    fn no_meta_refresh_returns_none() {
        assert_eq!(
            extract_refresh_url(
                "<html><head><title>x</title></head></html>",
                "https://e.com"
            ),
            None
        );
    }
}
