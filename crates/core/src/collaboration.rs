use crate::*;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccessRole {
    Owner,
    Editor,
    Commenter,
    Viewer,
}
impl AccessRole {
    pub fn edit(&self) -> bool {
        matches!(self, Self::Owner | Self::Editor)
    }
    pub fn comment(&self) -> bool {
        !matches!(self, Self::Viewer)
    }
    pub fn manage(&self) -> bool {
        matches!(self, Self::Owner)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub name: String,
    pub admin: bool,
    pub internal: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct OrganizationPolicy {
    pub canvas_enabled: bool,
    pub external_invites: bool,
}
impl Default for OrganizationPolicy {
    fn default() -> Self {
        Self {
            canvas_enabled: true,
            external_invites: false,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Me {
    pub user: Option<UserInfo>,
    pub auth_mode: String,
    pub policy: OrganizationPolicy,
    pub mail_available: bool,
    pub login_url: String,
    #[serde(default)]
    pub providers: Vec<LoginProvider>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoginProvider {
    pub label: String,
    pub login_url: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Member {
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub role: AccessRole,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct InvitationSummary {
    pub id: String,
    pub email: String,
    pub role: AccessRole,
    pub expires_at: i64,
    pub delivered: bool,
    pub accepted: bool,
    pub revoked: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Sharing {
    pub role: AccessRole,
    pub members: Vec<Member>,
    pub invitations: Vec<InvitationSummary>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InviteRequest {
    pub email: String,
    pub role: AccessRole,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Comment {
    pub id: String,
    pub author_id: String,
    pub author_name: String,
    pub body: String,
    pub created_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CommentThread {
    pub id: String,
    pub canvas_id: String,
    pub anchor_id: String,
    pub anchor_title: String,
    pub x: f64,
    pub y: f64,
    pub replies: Vec<Comment>,
    pub resolved: bool,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewThread {
    #[serde(default)]
    pub anchor_id: String,
    pub x: f64,
    pub y: f64,
    pub body: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub body: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolveThread {
    pub resolved: bool,
}
