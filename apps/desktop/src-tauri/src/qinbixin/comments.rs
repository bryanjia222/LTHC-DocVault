use serde_json::json;
use tauri::AppHandle;

use super::{
    environment::{load_environment, store_new_token},
    http::{absolute_url, request_json},
    types::{QinbixinComment, QinbixinResult, RawComment},
};
use crate::state::AppState;

#[tauri::command]
pub async fn qinbixin_message_comments(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    message_id: i64,
) -> Result<Vec<QinbixinComment>, String> {
    let environment = load_environment(&app);
    let token = state.qinbixin.lock().unwrap().token.clone();
    if token.is_empty() {
        return Err("AUTH_EXPIRED".to_owned());
    }
    let path =
        format!("/API/Web/Comment/GetCommentPageList?PageIndex=1&PageSize=50&WorksId={message_id}");
    let (envelope, new_token) = request_json::<Vec<RawComment>>(
        environment.base_url(),
        &token,
        reqwest::Method::GET,
        &path,
        None,
    )
    .await?;
    if !envelope.success.unwrap_or_default() {
        return Err(envelope.msg.unwrap_or_else(|| "request failed".to_owned()));
    }
    let comments = envelope.data.unwrap_or_default();
    store_new_token(&app, &state.qinbixin, &new_token)?;
    let base_url = environment.base_url();
    Ok(comments
        .into_iter()
        .map(|item| QinbixinComment {
            id: item.id,
            member_id: item.member_id,
            author: item
                .nickname
                .or(item.nick_name)
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "未知用户".to_owned()),
            avatar: absolute_url(base_url, item.avatar_url.unwrap_or_default()),
            content: item.content.unwrap_or_default(),
            sent_time: item.send_time.unwrap_or_default(),
            images: item
                .images
                .unwrap_or_default()
                .into_iter()
                .map(|url| absolute_url(base_url, url))
                .collect(),
        })
        .collect())
}

#[tauri::command]
pub async fn qinbixin_add_comment(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    message_id: i64,
    content: String,
    image_urls: Option<Vec<String>>,
) -> Result<QinbixinResult, String> {
    let environment = load_environment(&app);
    let token = state.qinbixin.lock().unwrap().token.clone();
    if token.is_empty() {
        return Err("AUTH_EXPIRED".to_owned());
    }
    let body = json!({
        "WorksId": message_id,
        "Content": content,
        "ImageUrl": image_urls.unwrap_or_default().join("*"),
    });
    let (envelope, new_token) = request_json::<serde_json::Value>(
        environment.base_url(),
        &token,
        reqwest::Method::POST,
        "/API/Web/Works/CreateComment",
        Some(body),
    )
    .await?;
    store_new_token(&app, &state.qinbixin, &new_token)?;
    Ok(QinbixinResult {
        success: envelope.success.unwrap_or_default(),
        message: envelope.msg.unwrap_or_default(),
    })
}
