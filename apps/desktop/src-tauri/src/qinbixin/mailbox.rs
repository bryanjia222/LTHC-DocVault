use std::sync::Mutex;

use tauri::AppHandle;

use super::{
    environment::{load_environment, store_new_token},
    http::{absolute_url, mapped_error, request_json},
    types::{
        QinbixinConversation, QinbixinEnvironment, QinbixinMessage, QinbixinSession,
        RawConversation, RawMessage,
    },
};
use crate::state::AppState;

pub(super) fn map_conversation(base_url: &str, raw: RawConversation) -> QinbixinConversation {
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

pub(super) async fn load_conversations(
    app: &AppHandle,
    state: &tauri::State<'_, AppState>,
) -> Result<Vec<QinbixinConversation>, String> {
    let environment = load_environment(app);
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
    store_new_token(app, &state.qinbixin, &token1)?;
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
    store_new_token(app, &state.qinbixin, &token2)?;
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

pub(super) async fn fetch_raw_messages(
    environment: &QinbixinEnvironment,
    app: &AppHandle,
    session: &Mutex<QinbixinSession>,
    token: &str,
    relationship_id: i64,
) -> Result<Vec<RawMessage>, String> {
    let path = format!(
        "/API/Web/Works/GetWorksPageList?PageIndex=1&PageSize=50&RelationshipId={relationship_id}"
    );
    let (envelope, new_token) = request_json::<Vec<RawMessage>>(
        environment.base_url(),
        token,
        reqwest::Method::GET,
        &path,
        None,
    )
    .await?;
    let raw = mapped_error(envelope)?;
    store_new_token(app, session, &new_token)?;
    Ok(raw)
}

pub(super) fn map_raw_messages(items: Vec<RawMessage>, base_url: &str) -> Vec<QinbixinMessage> {
    items
        .into_iter()
        .map(|item| QinbixinMessage {
            id: item.id,
            title: item.title.unwrap_or_default(),
            song_title: item.song_title.unwrap_or_default(),
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
            conversation_title: String::new(),
            sender_avatar: absolute_url(base_url, item.avatar_url.unwrap_or_default()),
            sent_time: item.send_time.unwrap_or_default(),
            images: item
                .images
                .unwrap_or_default()
                .into_iter()
                .map(|url| absolute_url(base_url, url))
                .collect(),
            videos: item
                .videos
                .unwrap_or_default()
                .into_iter()
                .map(|url| absolute_url(base_url, url))
                .collect(),
            file_url: absolute_url(base_url, item.file_url.unwrap_or_default()),
            tags: item.tags.unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::map_conversation;
    use crate::qinbixin::types::PRODUCTION_BASE_URL;

    #[test]
    fn maps_conversation_with_null_text_fields() {
        let conversation: crate::qinbixin::types::RawConversation = serde_json::from_str(
            r#"{"Id":728,"RelationshipType":1,"Title":"兰天","AvatarUrl":"avatar","Message":null}"#,
        )
        .unwrap();
        let mapped = map_conversation(PRODUCTION_BASE_URL, conversation);
        assert_eq!(mapped.title, "兰天");
        assert_eq!(mapped.preview, "");
    }
}
