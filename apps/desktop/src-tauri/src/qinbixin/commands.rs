use std::fs;

use serde_json::json;
use tauri::AppHandle;

use super::{
    environment::{load_environment, save_session, state_path, store_new_token},
    http::{absolute_url, mapped_error, request_json},
    mailbox::{fetch_raw_messages, load_conversations, map_raw_messages},
    types::{
        QinbixinConversation, QinbixinMedia, QinbixinMessage, QinbixinProfile, QinbixinResult,
        QinbixinSession, QinbixinStatusDto, RawLoginData, RawProfile,
    },
};
use crate::state::AppState;

pub(super) async fn login_with_credentials(
    app: &AppHandle,
    state: &tauri::State<'_, AppState>,
    user_name: String,
    password: String,
) -> Result<QinbixinStatusDto, String> {
    let environment = load_environment(app);
    let base_url = environment.base_url();
    let body = json!({ "UserName": user_name, "Password": password });
    let (envelope, _) = request_json::<RawLoginData>(
        base_url,
        "",
        reqwest::Method::POST,
        "/API/Web/Login",
        Some(body),
    )
    .await?;
    let data = mapped_error(envelope)?;
    let (profile_envelope, new_token) = request_json::<RawProfile>(
        base_url,
        &data.token,
        reqwest::Method::GET,
        "/API/Web/Member/GetMemberInfo",
        None,
    )
    .await?;
    let raw_profile = mapped_error(profile_envelope)?;
    let profile = QinbixinProfile {
        id: raw_profile.id,
        login_name: raw_profile.login_name.unwrap_or_default(),
        nickname: raw_profile.nickname.unwrap_or_default(),
        image_url: absolute_url(base_url, raw_profile.image_url.unwrap_or_default()),
    };
    {
        let mut guard = state.qinbixin.lock().unwrap();
        guard.token = data.token;
        guard.profile = profile.clone();
        if let Some(token) = &new_token {
            guard.token = token.clone();
        }
        let snapshot = guard.clone();
        drop(guard);
        save_session(app, &snapshot)?;
    }
    let has_unread = load_conversations(app, state)
        .await
        .map(|items| items.iter().any(|item| item.unread))
        .unwrap_or(false);
    Ok(QinbixinStatusDto {
        logged_in: true,
        profile: Some(profile),
        has_unread,
        environment,
    })
}

#[tauri::command]
pub async fn qinbixin_status(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<QinbixinStatusDto, String> {
    let environment = load_environment(&app);
    let session = {
        let guard = state.qinbixin.lock().unwrap();
        guard.clone()
    };
    if session.token.is_empty() {
        return Ok(QinbixinStatusDto {
            logged_in: false,
            profile: None,
            has_unread: false,
            environment,
        });
    }
    match load_conversations(&app, &state).await {
        Ok(conversations) => Ok(QinbixinStatusDto {
            logged_in: true,
            profile: Some(session.profile),
            has_unread: conversations.iter().any(|item| item.unread),
            environment,
        }),
        Err(e) if e == "AUTH_EXPIRED" => {
            let path = state_path(&app)?;
            let _ = fs::remove_file(path);
            *state.qinbixin.lock().unwrap() = QinbixinSession::default();
            Ok(QinbixinStatusDto {
                logged_in: false,
                profile: None,
                has_unread: false,
                environment,
            })
        }
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub async fn qinbixin_login(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    user_name: String,
    password: String,
) -> Result<QinbixinStatusDto, String> {
    login_with_credentials(&app, &state, user_name, password).await
}

#[tauri::command]
pub async fn qinbixin_logout(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let environment = load_environment(&app);
    let token = state.qinbixin.lock().unwrap().token.clone();
    if !token.is_empty() {
        let _ = request_json::<serde_json::Value>(
            environment.base_url(),
            &token,
            reqwest::Method::GET,
            "/API/Web/Exit?app=0",
            None,
        )
        .await;
    }
    let path = state_path(&app)?;
    let _ = fs::remove_file(path);
    *state.qinbixin.lock().unwrap() = QinbixinSession::default();
    Ok(())
}

#[tauri::command]
pub async fn qinbixin_conversations(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<QinbixinConversation>, String> {
    load_conversations(&app, &state).await
}

#[tauri::command]
pub async fn qinbixin_messages(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    relationship_id: i64,
) -> Result<Vec<QinbixinMessage>, String> {
    let environment = load_environment(&app);
    let token = state.qinbixin.lock().unwrap().token.clone();
    if token.is_empty() {
        return Err("AUTH_EXPIRED".to_owned());
    }
    let raw =
        fetch_raw_messages(&environment, &app, &state.qinbixin, &token, relationship_id).await?;
    Ok(map_raw_messages(
        raw,
        environment.base_url(),
        relationship_id,
    ))
}

#[tauri::command]
pub async fn qinbixin_inbox(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<QinbixinMessage>, String> {
    let environment = load_environment(&app);
    let conversations = load_conversations(&app, &state).await?;
    let mut messages = Vec::new();

    for conversation in conversations {
        let token = state.qinbixin.lock().unwrap().token.clone();
        if token.is_empty() {
            return Err("AUTH_EXPIRED".to_owned());
        }
        let raw_messages =
            fetch_raw_messages(&environment, &app, &state.qinbixin, &token, conversation.id)
                .await?;
        let mut mapped = map_raw_messages(raw_messages, environment.base_url(), conversation.id);
        for message in &mut mapped {
            message.conversation_title = conversation.title.clone();
        }
        messages.extend(mapped);
    }

    messages.sort_by_key(|message| std::cmp::Reverse(message.id));
    messages.dedup_by(|left, right| left.id == right.id);
    Ok(messages)
}

#[tauri::command]
pub async fn qinbixin_outbox(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<QinbixinMessage>, String> {
    let environment = load_environment(&app);
    let token = state.qinbixin.lock().unwrap().token.clone();
    if token.is_empty() {
        return Err("AUTH_EXPIRED".to_owned());
    }
    let path =
        "/API/Web/Works/GetSelfWorksPageList?PageIndex=1&PageSize=50&WorkType=5&WorkAuditType=15"
            .to_owned();
    let (envelope, new_token) = request_json::<Vec<super::types::RawMessage>>(
        environment.base_url(),
        &token,
        reqwest::Method::GET,
        &path,
        None,
    )
    .await?;
    let raw = mapped_error(envelope)?;
    store_new_token(&app, &state.qinbixin, &new_token)?;
    Ok(map_raw_messages(raw, environment.base_url(), 0))
}

#[tauri::command]
pub async fn qinbixin_send(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    relationship_id: i64,
    title: String,
    content: String,
    media: Option<QinbixinMedia>,
) -> Result<QinbixinResult, String> {
    let environment = load_environment(&app);
    let token = state.qinbixin.lock().unwrap().token.clone();
    if token.is_empty() {
        return Err("AUTH_EXPIRED".to_owned());
    }
    let media = media.unwrap_or_default();
    let body = json!({
        "Title": title,
        "Content": content,
        "ImageUrl": media.image_urls.join("*"),
        "VideoUrl": media.video_urls.join("*"),
        "FileUrl": media.file_urls.first().cloned().unwrap_or_default(),
        "WorkType": 5,
        "PublishType": 5,
        "RelationshipIds": [relationship_id],
    });
    let (envelope, new_token) = request_json::<serde_json::Value>(
        environment.base_url(),
        &token,
        reqwest::Method::POST,
        "/API/Web/Works/PushWorks",
        Some(body),
    )
    .await?;
    store_new_token(&app, &state.qinbixin, &new_token)?;
    Ok(QinbixinResult {
        success: envelope.success.unwrap_or_default(),
        message: envelope.msg.unwrap_or_default(),
    })
}

#[tauri::command]
pub async fn qinbixin_mark_read(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    relationship_id: i64,
) -> Result<(), String> {
    let environment = load_environment(&app);
    let token = state.qinbixin.lock().unwrap().token.clone();
    if token.is_empty() {
        return Err("AUTH_EXPIRED".to_owned());
    }
    let body = json!({ "Id": relationship_id, "IsGroup": false });
    let (_envelope, new_token) = request_json::<serde_json::Value>(
        environment.base_url(),
        &token,
        reqwest::Method::POST,
        "/API/Web/Relationship/FriendReaded",
        Some(body),
    )
    .await?;
    store_new_token(&app, &state.qinbixin, &new_token)?;
    Ok(())
}
