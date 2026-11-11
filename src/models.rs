//! Database models belong to the server; browser clients never receive credentials.
#[derive(Debug, toasty::Model)]
pub struct CanvasRecord {
    #[key]
    pub id: String,
    pub document: String,
    pub updated_at: String,
}
#[derive(Debug, toasty::Model)]
pub struct MutationRecord {
    #[key]
    pub id: String,
    #[index]
    pub canvas_id: String,
    pub revision: i64,
    pub body: String,
    pub created_at: String,
}
#[derive(Debug, toasty::Model)]
pub struct Metadata {
    #[key]
    pub id: String,
    pub value: String,
}

#[derive(Debug, toasty::Model)]
pub struct UserRecord {
    #[key]
    pub id: String,
    #[unique]
    pub email: String,
    pub document: String,
}
#[derive(Debug, toasty::Model)]
pub struct SessionRecord {
    #[key]
    pub id: String,
    #[index]
    pub user_id: String,
    pub document: String,
    pub expires_at: i64,
    pub kind: String,
}
#[derive(Debug, toasty::Model)]
pub struct GrantRecord {
    #[key]
    pub id: String,
    #[index]
    pub canvas_id: String,
    #[index]
    pub subject: String,
    pub role: String,
}
#[derive(Debug, toasty::Model)]
pub struct InvitationRecord {
    #[key]
    pub id: String,
    #[unique]
    pub token_hash: String,
    #[index]
    pub canvas_id: String,
    pub email: String,
    pub role: String,
    pub expires_at: i64,
    pub delivered: bool,
    pub accepted_by: String,
    pub revoked: bool,
}
#[derive(Debug, toasty::Model)]
pub struct CommentRecord {
    #[key]
    pub id: String,
    #[index]
    pub canvas_id: String,
    pub document: String,
}

#[derive(Debug, toasty::Model)]
pub struct IdentityRecord {
    #[key]
    pub id: String,
    #[index]
    pub user_id: String,
}
