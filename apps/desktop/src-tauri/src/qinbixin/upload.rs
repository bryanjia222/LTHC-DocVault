use std::{fs, path::PathBuf, sync::Mutex};

use serde_json::json;
use tauri::{AppHandle, Emitter};

use super::{
    environment::{load_environment, store_new_token},
    http::{absolute_url, mapped_error, mime_for_file, request_multipart},
    types::{QinbixinEnvironment, QinbixinSession, QinbixinUploadedFile, RawUploadedFile},
};
use crate::state::AppState;

const MAX_UPLOAD_BYTES: u64 = 100 * 1024 * 1024;
const THUMBNAIL_MAX_BYTES: u64 = 10 * 1024 * 1024;

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_owned()
}

fn base64_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = u32::from_be_bytes([0, b[0], b[1], b[2]]);
        out.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((n >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(n & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[tauri::command]
pub async fn qinbixin_thumbnail(path: String, kind: String) -> Result<Option<String>, String> {
    if !matches!(kind.as_str(), "image" | "video") {
        return Ok(None);
    }
    let file = PathBuf::from(&path);
    let metadata = fs::metadata(&file)
        .map_err(|e| crate::logging::log_error(format!("unable to stat media file: {e}")))?;
    if metadata.len() > THUMBNAIL_MAX_BYTES {
        return Ok(None);
    }
    let read_path = file.clone();
    let bytes = tauri::async_runtime::spawn_blocking(move || fs::read(read_path))
        .await
        .map_err(|e| crate::logging::log_error(format!("unable to read media file: {e}")))?
        .map_err(|e| crate::logging::log_error(format!("unable to read media file: {e}")))?;
    let mime = mime_for_file(&file_name(&file));
    let data_url = format!("data:{mime};base64,{}", base64_encode(&bytes));
    Ok(Some(data_url))
}

struct QinbixinUploadContext<'a> {
    app: &'a AppHandle,
    session: &'a Mutex<QinbixinSession>,
    environment: QinbixinEnvironment,
    token: String,
}

async fn upload_media_bytes(
    context: QinbixinUploadContext<'_>,
    file_name: String,
    bytes: Vec<u8>,
    upload_type: u8,
    index: usize,
) -> Result<QinbixinUploadedFile, String> {
    let QinbixinUploadContext {
        app,
        session,
        environment,
        token,
    } = context;
    if bytes.is_empty() {
        return Err(crate::logging::log_warn(format!(
            "文件 {file_name} 容量为0KB"
        )));
    }
    if bytes.len() as u64 > MAX_UPLOAD_BYTES {
        return Err(crate::logging::log_warn(format!(
            "文件 {file_name} 超过上传大小限制"
        )));
    }
    let upload_path = format!("/API/Web/Upload?uploadType={upload_type}");
    let app_for_progress = app.clone();
    let file_for_progress = file_name.clone();
    let on_progress = move |sent: usize, total: usize| {
        let percent = if total == 0 {
            100
        } else {
            ((sent as u64 * 100) / total as u64) as i32
        };
        if let Err(error) = app_for_progress.emit(
            "qinbixin-upload-progress",
            json!({
                "index": index,
                "fileName": file_for_progress,
                "percent": percent,
            }),
        ) {
            tracing::warn!(
                index,
                file_name = %file_for_progress,
                error = %error,
                "failed to emit qinbixin upload progress"
            );
        }
    };
    let (envelope, new_token) = request_multipart::<RawUploadedFile>(
        environment.base_url(),
        &token,
        &upload_path,
        &file_name,
        bytes,
        Some(Box::new(on_progress)),
    )
    .await?;
    let raw = mapped_error(envelope)?;
    let uploaded = QinbixinUploadedFile {
        url: absolute_url(environment.base_url(), raw.location),
        title: raw.title.unwrap_or_else(|| file_name.clone()),
    };
    store_new_token(app, session, &new_token)?;
    Ok(uploaded)
}

#[tauri::command]
pub async fn qinbixin_upload(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    paths: Vec<String>,
    upload_type: u8,
) -> Result<Vec<QinbixinUploadedFile>, String> {
    if !matches!(upload_type, 0..=2) {
        return Err(crate::logging::log_warn("invalid media type"));
    }
    let environment = load_environment(&app);
    let token = state.qinbixin.lock().unwrap().token.clone();
    if token.is_empty() {
        return Err(crate::logging::log_warn("AUTH_EXPIRED"));
    }
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut uploaded = Vec::with_capacity(paths.len());
    for (index, path_text) in paths.iter().enumerate() {
        let path = PathBuf::from(&path_text);
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| crate::logging::log_warn("invalid media file"))?
            .to_owned();
        let read_path = path.clone();
        let bytes = tauri::async_runtime::spawn_blocking(move || fs::read(read_path))
            .await
            .map_err(|e| crate::logging::log_error(format!("unable to read media file: {e}")))?
            .map_err(|e| crate::logging::log_error(format!("unable to read media file: {e}")))?;
        uploaded.push(
            upload_media_bytes(
                QinbixinUploadContext {
                    app: &app,
                    session: &state.qinbixin,
                    environment,
                    token: token.clone(),
                },
                file_name,
                bytes,
                upload_type,
                index,
            )
            .await?,
        );
    }
    Ok(uploaded)
}

#[tauri::command]
pub async fn qinbixin_upload_bytes(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    file_name: String,
    bytes: Vec<u8>,
    upload_type: u8,
) -> Result<QinbixinUploadedFile, String> {
    if !matches!(upload_type, 0..=2) {
        return Err(crate::logging::log_warn("invalid media type"));
    }
    let file_name = std::path::Path::new(&file_name)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| crate::logging::log_warn("invalid media file"))?
        .to_owned();
    let environment = load_environment(&app);
    let token = state.qinbixin.lock().unwrap().token.clone();
    if token.is_empty() {
        return Err(crate::logging::log_warn("AUTH_EXPIRED"));
    }
    upload_media_bytes(
        QinbixinUploadContext {
            app: &app,
            session: &state.qinbixin,
            environment,
            token,
        },
        file_name,
        bytes,
        upload_type,
        0,
    )
    .await
}
