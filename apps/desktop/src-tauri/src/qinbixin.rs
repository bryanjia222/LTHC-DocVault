use std::{
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::Duration,
};

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_rustls::HttpsConnector;
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::{TokioExecutor, TokioTimer},
};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Manager};

const PRODUCTION_BASE_URL: &str = "https://qinbixin.com.cn";
const TEST_BASE_URL: &str = "http://admin.ymcs.top:928";
const PRODUCTION_STATE_FILE: &str = "qinbixin-state.json";
const TEST_STATE_FILE: &str = "qinbixin-state-test.json";
const ENVIRONMENT_FILE: &str = "qinbixin-environment.json";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QinbixinEnvironment {
    #[default]
    Production,
    Test,
}

impl QinbixinEnvironment {
    fn base_url(self) -> &'static str {
        match self {
            Self::Production => PRODUCTION_BASE_URL,
            Self::Test => TEST_BASE_URL,
        }
    }

    fn state_file(self) -> &'static str {
        match self {
            Self::Production => PRODUCTION_STATE_FILE,
            Self::Test => TEST_STATE_FILE,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QinbixinProfile {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub login_name: String,
    #[serde(default)]
    pub nickname: String,
    #[serde(default)]
    pub image_url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QinbixinSession {
    pub token: String,
    pub profile: QinbixinProfile,
}

#[derive(Debug, Serialize)]
pub struct QinbixinStatusDto {
    pub logged_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<QinbixinProfile>,
    pub has_unread: bool,
    pub environment: QinbixinEnvironment,
}

#[derive(Debug, Clone, Serialize)]
pub struct QinbixinConversation {
    pub id: i64,
    pub title: String,
    pub avatar: String,
    pub is_group: bool,
    pub unread: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QinbixinMessage {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub sender_id: i64,
    pub sender_name: String,
    pub sender_avatar: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sent_time: String,
}

#[derive(Debug, Serialize)]
pub struct QinbixinResult {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(not(debug_assertions), allow(dead_code))]
pub struct QinbixinDevAccount {
    pub id: String,
    pub user_name: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct QinbixinEnvironmentFile {
    environment: QinbixinEnvironment,
}

#[derive(Debug, Deserialize)]
struct RawEnvelope<T> {
    success: Option<bool>,
    msg: Option<String>,
    data: Option<T>,
}

#[derive(Debug, Deserialize)]
struct RawLoginData {
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawProfile {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    login_name: Option<String>,
    #[serde(default)]
    nickname: Option<String>,
    #[serde(default)]
    image_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawUnreadFlags {
    #[serde(default)]
    flag_friend: i64,
    #[serde(default)]
    flag_group: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawConversation {
    id: i64,
    #[serde(default)]
    relationship_type: i64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    is_unread: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawMessage {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    member_id: i64,
    #[serde(default)]
    nick_name: Option<String>,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(default)]
    send_time: Option<String>,
}

fn environment_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(ENVIRONMENT_FILE))
        .map_err(|e| format!("unable to resolve app config dir: {e}"))
}

fn load_environment(app: &AppHandle) -> QinbixinEnvironment {
    let Ok(path) = environment_path(app) else {
        return QinbixinEnvironment::default();
    };
    let Ok(text) = fs::read_to_string(path) else {
        return QinbixinEnvironment::default();
    };
    if let Ok(value) = serde_json::from_str::<QinbixinEnvironmentFile>(&text) {
        return value.environment;
    }
    QinbixinEnvironment::default()
}

#[cfg_attr(not(debug_assertions), allow(dead_code))]
fn save_environment(app: &AppHandle, environment: QinbixinEnvironment) -> Result<(), String> {
    let path = environment_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("unable to create config dir: {e}"))?;
    }
    let text = serde_json::to_string(&QinbixinEnvironmentFile { environment })
        .map_err(|e| format!("unable to encode environment: {e}"))?;
    fs::write(path, text).map_err(|e| format!("unable to save environment: {e}"))
}

fn state_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(load_environment(app).state_file()))
        .map_err(|e| format!("unable to resolve app config dir: {e}"))
}

fn read_session(path: &Path) -> Option<QinbixinSession> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn load_session(app: &AppHandle, session: &Mutex<QinbixinSession>) {
    let Ok(path) = state_path(app) else {
        return;
    };
    if let Some(value) = read_session(&path) {
        *session.lock().unwrap() = value;
    }
}

fn save_session(app: &AppHandle, session: &QinbixinSession) -> Result<(), String> {
    let path = state_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("unable to create config dir: {e}"))?;
    }
    let text =
        serde_json::to_string(session).map_err(|e| format!("unable to encode session: {e}"))?;
    fs::write(path, text).map_err(|e| format!("unable to save session: {e}"))
}

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

type QinbixinHttpClient = Client<HttpsConnector<HttpConnector>, Full<Bytes>>;

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

async fn request_json<T: serde::de::DeserializeOwned>(
    base_url: &str,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> Result<(RawEnvelope<T>, Option<String>), String> {
    let uri = format!("{base_url}{path}")
        .parse::<hyper::Uri>()
        .map_err(|e| format!("invalid request URL: {e}"))?;
    let request = hyper::Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"))
        .header("accept", "*/*")
        .header("user-agent", "DocVault/0.2");
    let request = if let Some(body) = body {
        let text =
            serde_json::to_vec(&body).map_err(|e| format!("unable to encode request: {e}"))?;
        request
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from(text)))
    } else {
        request.body(Full::new(Bytes::new()))
    }
    .map_err(|e| format!("unable to create request: {e}"))?;
    let response = tokio::time::timeout(Duration::from_secs(10), HTTP_CLIENT.request(request))
        .await
        .map_err(|_| "request timed out".to_owned())?
        .map_err(|e| format!("request failed: {}", format_error_chain(e)))?;
    let new_token = response
        .headers()
        .get("NewToken")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .map_err(|e| format!("unable to read response: {}", format_error_chain(e)))?;
    let text = String::from_utf8_lossy(bytes.to_bytes().as_ref()).into_owned();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err("AUTH_EXPIRED".to_owned());
    }
    serde_json::from_str(&text)
        .map(|envelope| (envelope, new_token))
        .map_err(|e| format!("invalid response: {e}; body: {text}"))
}

fn mapped_error<T>(envelope: RawEnvelope<T>) -> Result<T, String> {
    if envelope.success.unwrap_or_default() {
        envelope
            .data
            .ok_or_else(|| "missing response data".to_owned())
    } else {
        Err(envelope.msg.unwrap_or_else(|| "request failed".to_owned()))
    }
}

fn absolute_url(base_url: &str, url: String) -> String {
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

fn store_new_token(
    app: &AppHandle,
    session: &Mutex<QinbixinSession>,
    token: &Option<String>,
) -> Result<(), String> {
    if let Some(new_token) = token {
        let mut guard = session.lock().unwrap();
        if *new_token != guard.token {
            guard.token = new_token.clone();
            let snapshot = guard.clone();
            drop(guard);
            save_session(app, &snapshot)?;
        }
    }
    Ok(())
}

fn map_conversation(base_url: &str, raw: RawConversation) -> QinbixinConversation {
    let title = raw.title.unwrap_or_default();
    QinbixinConversation {
        id: raw.id,
        title: if title.is_empty() {
            "未命名会话".to_owned()
        } else {
            title
        },
        avatar: absolute_url(base_url, raw.avatar_url.unwrap_or_default()),
        is_group: raw.relationship_type == 5,
        unread: raw.is_unread,
        preview: raw.message.unwrap_or_default(),
    }
}

async fn load_unread(base_url: &str, token: &str) -> Result<bool, String> {
    let (envelope, _) = request_json::<RawUnreadFlags>(
        base_url,
        token,
        reqwest::Method::GET,
        "/API/Web/Relationship/GetFriendRequestUnreadFlag",
        None,
    )
    .await?;
    let flags = mapped_error(envelope)?;
    Ok(flags.flag_friend > 0 || flags.flag_group > 0)
}

#[tauri::command]
pub async fn qinbixin_status(
    app: AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
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
    match load_unread(environment.base_url(), &session.token).await {
        Ok(has_unread) => Ok(QinbixinStatusDto {
            logged_in: true,
            profile: Some(session.profile),
            has_unread,
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

async fn login_with_credentials(
    app: &AppHandle,
    state: &tauri::State<'_, crate::state::AppState>,
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
    let has_unread = load_unread(base_url, &data.token).await.unwrap_or(false);
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
    Ok(QinbixinStatusDto {
        logged_in: true,
        profile: Some(profile),
        has_unread,
        environment,
    })
}

#[tauri::command]
pub async fn qinbixin_login(
    app: AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    user_name: String,
    password: String,
) -> Result<QinbixinStatusDto, String> {
    login_with_credentials(&app, &state, user_name, password).await
}

#[tauri::command]
pub async fn qinbixin_logout(
    app: AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
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
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Vec<QinbixinConversation>, String> {
    let environment = load_environment(&app);
    let base_url = environment.base_url();
    let session = state.qinbixin.lock().unwrap().clone();
    let token = session.token;
    if token.is_empty() {
        return Err("AUTH_EXPIRED".to_owned());
    }
    let (friend_envelope, token1) = request_json::<Vec<RawConversation>>(
        base_url,
        &token,
        reqwest::Method::GET,
        "/API/Web/Relationship/GetPageList?PageIndex=1&PageSize=200&IsFirend=true",
        None,
    )
    .await?;
    let friends = mapped_error(friend_envelope)?;
    store_new_token(&app, &state.qinbixin, &token1)?;
    let active_token = token1.unwrap_or(token);
    let (group_envelope, token2) = request_json::<Vec<RawConversation>>(
        base_url,
        &active_token,
        reqwest::Method::GET,
        "/API/Web/Relationship/GetPageList?PageIndex=1&PageSize=200&IsFirend=false",
        None,
    )
    .await?;
    let groups = mapped_error(group_envelope)?;
    store_new_token(&app, &state.qinbixin, &token2)?;
    let mut conversations: Vec<_> = friends
        .into_iter()
        .map(|item| map_conversation(base_url, item))
        .chain(
            groups
                .into_iter()
                .map(|item| map_conversation(base_url, item)),
        )
        .collect();
    conversations.sort_by_key(|item| std::cmp::Reverse(item.unread));
    Ok(conversations)
}

#[tauri::command]
pub async fn qinbixin_messages(
    app: AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    relationship_id: i64,
) -> Result<Vec<QinbixinMessage>, String> {
    let environment = load_environment(&app);
    let token = state.qinbixin.lock().unwrap().token.clone();
    if token.is_empty() {
        return Err("AUTH_EXPIRED".to_owned());
    }
    let path = format!(
        "/API/Web/Works/GetWorksPageList?PageIndex=1&PageSize=50&RelationshipId={relationship_id}"
    );
    let (envelope, new_token) = request_json::<Vec<RawMessage>>(
        environment.base_url(),
        &token,
        reqwest::Method::GET,
        &path,
        None,
    )
    .await?;
    let raw = mapped_error(envelope)?;
    store_new_token(&app, &state.qinbixin, &new_token)?;
    Ok(raw
        .into_iter()
        .map(|item| QinbixinMessage {
            id: item.id,
            title: item.title.unwrap_or_default(),
            content: item.content.unwrap_or_default(),
            sender_id: item.member_id,
            sender_name: {
                let nick_name = item.nick_name.unwrap_or_default();
                if nick_name.is_empty() {
                    "未知发送者".to_owned()
                } else {
                    nick_name
                }
            },
            sender_avatar: absolute_url(
                environment.base_url(),
                item.avatar_url.unwrap_or_default(),
            ),
            sent_time: item.send_time.unwrap_or_default(),
        })
        .collect())
}

#[tauri::command]
pub async fn qinbixin_send(
    app: AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    relationship_id: i64,
    title: String,
    content: String,
) -> Result<QinbixinResult, String> {
    let environment = load_environment(&app);
    let token = state.qinbixin.lock().unwrap().token.clone();
    if token.is_empty() {
        return Err("AUTH_EXPIRED".to_owned());
    }
    let paragraphs = content
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| format!("<p>{}</p>", line))
        .collect::<String>();
    let body = json!({
        "Title": title,
        "Content": paragraphs,
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
    state: tauri::State<'_, crate::state::AppState>,
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

#[cfg_attr(not(debug_assertions), allow(dead_code))]
fn status_snapshot(
    environment: QinbixinEnvironment,
    session: &QinbixinSession,
) -> QinbixinStatusDto {
    QinbixinStatusDto {
        logged_in: !session.token.is_empty(),
        profile: if session.token.is_empty() {
            None
        } else {
            Some(session.profile.clone())
        },
        has_unread: false,
        environment,
    }
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn qinbixin_set_environment(
    app: AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    environment: QinbixinEnvironment,
) -> Result<QinbixinStatusDto, String> {
    let current = load_environment(&app);
    if current != environment {
        save_environment(&app, environment)?;
        let path = state_path(&app)?;
        *state.qinbixin.lock().unwrap() = read_session(&path).unwrap_or_default();
    }
    Ok(status_snapshot(
        environment,
        &state.qinbixin.lock().unwrap(),
    ))
}

#[cfg(debug_assertions)]
fn dotenv_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.env")
}

#[cfg(debug_assertions)]
fn read_env_map() -> Result<std::collections::HashMap<String, String>, String> {
    let text = fs::read_to_string(dotenv_path())
        .map_err(|e| format!("unable to read dev credentials: {e}"))?;
    Ok(text
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((
                key.trim().to_owned(),
                value.trim().trim_matches('"').to_owned(),
            ))
        })
        .collect())
}

#[cfg(debug_assertions)]
fn parse_dev_accounts(env: &std::collections::HashMap<String, String>) -> Vec<QinbixinDevAccount> {
    (1..=3)
        .filter_map(|index| {
            let id = index.to_string();
            let user_name = env
                .get(&format!("DEV_QBX_ID_{index}"))
                .map(|value| value.trim())
                .unwrap_or_default();
            let password = env
                .get(&format!("DEV_QBX_PASSWORD_{index}"))
                .map(|value| value.trim())
                .unwrap_or_default();
            (!user_name.is_empty() && !password.is_empty()).then(|| QinbixinDevAccount {
                id,
                user_name: user_name.to_owned(),
            })
        })
        .collect()
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn qinbixin_dev_accounts() -> Result<Vec<QinbixinDevAccount>, String> {
    Ok(parse_dev_accounts(&read_env_map()?))
}

#[cfg(debug_assertions)]
#[tauri::command]
pub async fn qinbixin_login_dev_account(
    app: AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
    account_id: String,
) -> Result<QinbixinStatusDto, String> {
    let env = read_env_map()?;
    let index = account_id
        .parse::<usize>()
        .map_err(|_| "invalid dev account".to_owned())?;
    let user_name = env
        .get(&format!("DEV_QBX_ID_{index}"))
        .map(|value| value.trim())
        .unwrap_or_default();
    let password = env
        .get(&format!("DEV_QBX_PASSWORD_{index}"))
        .map(|value| value.trim())
        .unwrap_or_default();
    if user_name.is_empty() || password.is_empty() {
        return Err("missing dev account credentials".to_owned());
    }
    login_with_credentials(&app, &state, user_name.to_owned(), password.to_owned()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_pascal_case_profile() {
        let profile: RawProfile =
            serde_json::from_str(r#"{"Id":7,"LoginName":"demo","Nickname":"Demo","ImageUrl":"x"}"#)
                .unwrap();
        assert_eq!(profile.id, 7);
        assert_eq!(profile.login_name.as_deref(), Some("demo"));
    }

    #[test]
    fn maps_lowercase_envelope() {
        let envelope: RawEnvelope<RawLoginData> = serde_json::from_str(
            r#"{"code":0,"data":{"token":"demo-token"},"msg":"ok","success":true}"#,
        )
        .unwrap();
        assert!(envelope.success.unwrap());
        assert_eq!(envelope.data.unwrap().token, "demo-token");
    }

    #[test]
    fn maps_pascal_case_message() {
        let message: RawMessage =
            serde_json::from_str(r#"{"Id":1,"Content":"<p>hi</p>","NickName":"Demo"}"#).unwrap();
        assert_eq!(message.id, 1);
        assert_eq!(message.nick_name.as_deref(), Some("Demo"));
    }

    #[test]
    fn maps_conversation_with_null_text_fields() {
        let conversation: RawConversation = serde_json::from_str(
            r#"{"Id":728,"RelationshipType":1,"Title":"兰天","AvatarUrl":"avatar","Message":null}"#,
        )
        .unwrap();
        let mapped = map_conversation(PRODUCTION_BASE_URL, conversation);
        assert_eq!(mapped.title, "兰天");
        assert_eq!(mapped.preview, "");
    }

    #[test]
    fn resolves_relative_asset_urls() {
        assert_eq!(
            absolute_url(PRODUCTION_BASE_URL, "/uploadfiles/avatar.jpg".to_owned()),
            "https://qinbixin.com.cn/uploadfiles/avatar.jpg"
        );
        assert_eq!(
            absolute_url(PRODUCTION_BASE_URL, "uploadfiles/avatar.jpg".to_owned()),
            "https://qinbixin.com.cn/uploadfiles/avatar.jpg"
        );
        assert_eq!(
            absolute_url(
                PRODUCTION_BASE_URL,
                "https://qinbixin.com.cn/avatar.jpg".to_owned()
            ),
            "https://qinbixin.com.cn/avatar.jpg"
        );
    }
}
