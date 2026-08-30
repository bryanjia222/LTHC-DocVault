use serde::{Deserialize, Serialize};

pub(super) const PRODUCTION_BASE_URL: &str = "https://qinbixin.com.cn";
pub(super) const TEST_BASE_URL: &str = "http://admin.ymcs.top:928";
const PRODUCTION_STATE_FILE: &str = "qinbixin-state.json";
const TEST_STATE_FILE: &str = "qinbixin-state-test.json";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QinbixinEnvironment {
    #[default]
    Production,
    Test,
}

impl QinbixinEnvironment {
    pub(super) fn base_url(self) -> &'static str {
        match self {
            Self::Production => PRODUCTION_BASE_URL,
            Self::Test => TEST_BASE_URL,
        }
    }

    pub(super) fn state_file(self) -> &'static str {
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
    pub song_title: String,
    pub content: String,
    pub sender_id: i64,
    pub sender_name: String,
    pub sender_avatar: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub conversation_title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sent_time: String,
    #[serde(default)]
    pub images: Vec<String>,
    #[serde(default)]
    pub videos: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub file_url: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub comment_count: i64,
    #[serde(default)]
    pub relationship_id: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct QinbixinComment {
    pub id: i64,
    pub member_id: i64,
    pub author: String,
    pub avatar: String,
    pub content: String,
    pub sent_time: String,
    #[serde(default)]
    pub images: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QinbixinUploadedFile {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QinbixinMedia {
    #[serde(default)]
    pub image_urls: Vec<String>,
    #[serde(default)]
    pub video_urls: Vec<String>,
    #[serde(default)]
    pub file_urls: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct QinbixinResult {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[cfg(debug_assertions)]
pub struct QinbixinDevAccount {
    pub id: String,
    pub user_name: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct QinbixinEnvironmentFile {
    pub(super) environment: QinbixinEnvironment,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawEnvelope<T> {
    pub(super) success: Option<bool>,
    pub(super) msg: Option<String>,
    pub(super) data: Option<T>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawLoginData {
    pub(super) token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct RawProfile {
    #[serde(default)]
    pub(super) id: i64,
    #[serde(default)]
    pub(super) login_name: Option<String>,
    #[serde(default)]
    pub(super) nickname: Option<String>,
    #[serde(default)]
    pub(super) image_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct RawConversation {
    pub(super) id: i64,
    #[serde(default)]
    pub(super) relationship_type: i64,
    #[serde(default)]
    pub(super) title: Option<String>,
    #[serde(default)]
    pub(super) avatar_url: Option<String>,
    #[serde(default)]
    pub(super) message: Option<String>,
    #[serde(default)]
    pub(super) is_unread: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct RawMessage {
    #[serde(default)]
    pub(super) id: i64,
    #[serde(default)]
    pub(super) title: Option<String>,
    #[serde(default)]
    pub(super) song_title: Option<String>,
    #[serde(default)]
    pub(super) content: Option<String>,
    #[serde(default)]
    pub(super) member_id: i64,
    #[serde(default)]
    pub(super) nick_name: Option<String>,
    #[serde(default)]
    pub(super) avatar_url: Option<String>,
    #[serde(default)]
    pub(super) send_time: Option<String>,
    #[serde(default)]
    pub(super) images: Option<Vec<String>>,
    #[serde(default)]
    pub(super) videos: Option<Vec<String>>,
    #[serde(default)]
    pub(super) file_url: Option<String>,
    #[serde(default)]
    pub(super) tags: Option<Vec<String>>,
    #[serde(default)]
    pub(super) comment_qty: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct RawComment {
    #[serde(default)]
    pub(super) id: i64,
    #[serde(default)]
    pub(super) member_id: i64,
    #[serde(default)]
    pub(super) avatar_url: Option<String>,
    #[serde(default)]
    pub(super) nickname: Option<String>,
    #[serde(default)]
    pub(super) nick_name: Option<String>,
    #[serde(default)]
    pub(super) content: Option<String>,
    #[serde(default)]
    pub(super) send_time: Option<String>,
    #[serde(default)]
    pub(super) images: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RawUploadedFile {
    pub(super) location: String,
    #[serde(default)]
    pub(super) title: Option<String>,
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
    fn maps_message_media_fields() {
        let message: RawMessage = serde_json::from_str(
            r#"{
                "Id": 2,
                "SongTitle": "Demo",
                "Images": ["/images/demo.png"],
                "Videos": ["/videos/demo.mp4"],
                "FileUrl": "/files/demo.pdf",
                "Tags": ["demo"]
            }"#,
        )
        .unwrap();
        assert_eq!(message.song_title.as_deref(), Some("Demo"));
        assert_eq!(message.images, Some(vec!["/images/demo.png".to_owned()]));
        assert_eq!(message.videos, Some(vec!["/videos/demo.mp4".to_owned()]));
        assert_eq!(message.file_url.as_deref(), Some("/files/demo.pdf"));
        assert_eq!(message.tags, Some(vec!["demo".to_owned()]));
    }

    #[test]
    fn maps_comment_fields() {
        let comment: RawComment = serde_json::from_str(
            r#"{
                "Id": 9,
                "MemberId": 42,
                "Nickname": "Demo",
                "Content": "reply",
                "Images": ["/images/reply.png"]
            }"#,
        )
        .unwrap();
        assert_eq!(comment.id, 9);
        assert_eq!(comment.nickname.as_deref(), Some("Demo"));
        assert_eq!(comment.images, Some(vec!["/images/reply.png".to_owned()]));
    }
}
