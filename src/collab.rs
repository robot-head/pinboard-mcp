use crate::{
    models::*,
    store::{Change, Result, Store, StoreError},
};
use pinboard_core::*;
use toasty_core::driver::operation::TransactionMode;

pub fn local_user() -> UserInfo {
    UserInfo {
        id: "local".into(),
        email: "local@localhost".into(),
        name: "You".into(),
        admin: true,
        internal: true,
    }
}
fn role_string(role: &AccessRole) -> String {
    serde_json::to_string(role).unwrap()
}
fn parse_role(s: &str) -> Result<AccessRole> {
    Ok(serde_json::from_str(s)?)
}
fn forbidden(message: &str) -> StoreError {
    StoreError::Forbidden(message.into())
}
fn invalid(message: &str) -> StoreError {
    CanvasError::Invalid(message.into()).into()
}
fn timestamp() -> i64 {
    chrono::Utc::now().timestamp()
}
pub fn random_token() -> String {
    format!("{}{}", new_id(), new_id())
}
pub(crate) enum WritePermission {
    Edit,
    Comment,
    Manage,
}
async fn policy_in(tx: &mut toasty::Transaction<'_>) -> Result<OrganizationPolicy> {
    Ok(Metadata::filter_by_id("organization_policy")
        .first()
        .exec(tx)
        .await?
        .map(|m| serde_json::from_str(&m.value))
        .transpose()?
        .unwrap_or_default())
}
pub(crate) async fn authorize_write(
    tx: &mut toasty::Transaction<'_>,
    user: &UserInfo,
    id: &str,
    needed: WritePermission,
) -> Result<AccessRole> {
    let policy = policy_in(tx).await?;
    if !policy.canvas_enabled {
        return Err(forbidden(
            "The canvas feature is disabled by your organization",
        ));
    }
    let role = if user.admin {
        AccessRole::Owner
    } else {
        let grant = GrantRecord::filter_by_id(format!("{id}:{}", user.id))
            .first()
            .exec(tx)
            .await?
            .ok_or_else(|| forbidden("You do not have access to this canvas"))?;
        parse_role(&grant.role)?
    };
    if match needed {
        WritePermission::Edit => !role.edit(),
        WritePermission::Comment => !role.comment(),
        WritePermission::Manage => !role.manage(),
    } {
        return Err(forbidden("Your canvas role does not permit this change"));
    }
    Ok(role)
}

#[derive(Clone)]
pub struct Workspace {
    pub store: Store,
    pub user: UserInfo,
}
impl Workspace {
    pub fn new(store: Store, user: UserInfo) -> Self {
        Self { store, user }
    }
    pub async fn role(&self, id: &str) -> Result<AccessRole> {
        if !self.store.policy().await?.canvas_enabled {
            return Err(forbidden(
                "The canvas feature is disabled by your organization",
            ));
        }
        if self.user.admin {
            self.store.get(id).await?;
            return Ok(AccessRole::Owner);
        }
        let grant = GrantRecord::filter_by_id(format!("{id}:{}", self.user.id))
            .first()
            .exec(&mut self.store.db.clone())
            .await?
            .ok_or_else(|| forbidden("You do not have access to this canvas"))?;
        parse_role(&grant.role)
    }
    pub async fn read(&self, id: &str) -> Result<Canvas> {
        self.role(id).await?;
        self.store.get(id).await
    }
    pub async fn list(&self) -> Result<Vec<CanvasSummary>> {
        if !self.store.policy().await?.canvas_enabled {
            return Err(forbidden(
                "The canvas feature is disabled by your organization",
            ));
        }
        let all = self.store.list().await?;
        if self.user.admin {
            return Ok(all);
        }
        let grants = GrantRecord::filter_by_subject(&self.user.id)
            .exec(&mut self.store.db.clone())
            .await?;
        Ok(all
            .into_iter()
            .filter(|c| grants.iter().any(|g| g.canvas_id == c.id))
            .collect())
    }
    pub async fn create(&self, title: &str, nodes: Vec<Node>, edges: Vec<Edge>) -> Result<Canvas> {
        if !self.store.policy().await?.canvas_enabled {
            return Err(forbidden(
                "The canvas feature is disabled by your organization",
            ));
        }
        if !self.user.internal && !self.user.admin {
            return Err(forbidden(
                "Guests can open invited canvases; only workspace members can create canvases",
            ));
        }
        let mut c = Canvas::new(title);
        c.nodes = nodes;
        c.edges = edges;
        c.validate()?;
        let _writer = self.store.write_lock.lock().await;
        let mut db = self.store.db.clone();
        let mut tx = db
            .transaction_builder()
            .mode(TransactionMode::Immediate)
            .begin()
            .await?;
        if !policy_in(&mut tx).await?.canvas_enabled {
            return Err(forbidden(
                "The canvas feature is disabled by your organization",
            ));
        }
        toasty::create!(CanvasRecord {
            id: c.id.clone(),
            document: serde_json::to_string(&c)?,
            updated_at: c.updated_at.clone()
        })
        .exec(&mut tx)
        .await?;
        toasty::create!(GrantRecord {
            id: format!("{}:{}", c.id, self.user.id),
            canvas_id: c.id.clone(),
            subject: self.user.id.clone(),
            role: role_string(&AccessRole::Owner)
        })
        .exec(&mut tx)
        .await?;
        tx.commit().await?;
        Ok(c)
    }
    pub async fn edit(&self, id: &str, batch: &Mutation) -> Result<Canvas> {
        if !self.role(id).await?.edit() {
            return Err(forbidden("Your canvas role is read-only"));
        }
        self.store.mutate_as(&self.user, id, batch).await
    }
    pub async fn sharing(&self, id: &str) -> Result<Sharing> {
        let role = self.role(id).await?;
        if !role.manage() {
            return Err(forbidden("Only canvas owners can manage sharing"));
        }
        let mut db = self.store.db.clone();
        let grants = GrantRecord::filter_by_canvas_id(id).exec(&mut db).await?;
        let mut members = vec![];
        for grant in grants {
            let user = UserRecord::filter_by_id(&grant.subject)
                .first()
                .exec(&mut db)
                .await?
                .map(|r| serde_json::from_str::<UserInfo>(&r.document))
                .transpose()?
                .unwrap_or_else(local_user);
            members.push(Member {
                user_id: grant.subject,
                email: user.email,
                name: user.name,
                role: parse_role(&grant.role)?,
            });
        }
        let invitations = InvitationRecord::filter_by_canvas_id(id)
            .exec(&mut db)
            .await?
            .into_iter()
            .map(|i| {
                Ok(InvitationSummary {
                    id: i.id,
                    email: i.email,
                    role: parse_role(&i.role)?,
                    expires_at: i.expires_at,
                    delivered: i.delivered,
                    accepted: !i.accepted_by.is_empty(),
                    revoked: i.revoked,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Sharing {
            role,
            members,
            invitations,
        })
    }
    pub async fn revoke_member(&self, id: &str, subject: &str) -> Result<()> {
        if !self.role(id).await?.manage() {
            return Err(forbidden("Only canvas owners can manage sharing"));
        }
        let _writer = self.store.write_lock.lock().await;
        let mut db = self.store.db.clone();
        let mut tx = db
            .transaction_builder()
            .mode(TransactionMode::Immediate)
            .begin()
            .await?;
        authorize_write(&mut tx, &self.user, id, WritePermission::Manage).await?;
        let key = format!("{id}:{subject}");
        let grant = GrantRecord::filter_by_id(&key)
            .first()
            .exec(&mut tx)
            .await?
            .ok_or_else(|| CanvasError::NotFound(subject.into()))?;
        if parse_role(&grant.role)?.manage() {
            return Err(invalid("The owner cannot be removed"));
        }
        GrantRecord::filter_by_id(key)
            .delete()
            .exec(&mut tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn prepare_invitation(
        &self,
        id: &str,
        p: InviteRequest,
        internal_domains: &[String],
    ) -> Result<(InvitationSummary, String, String)> {
        if !self.role(id).await?.manage() {
            return Err(forbidden("Only canvas owners can invite collaborators"));
        }
        if p.role.manage() {
            return Err(invalid("Invite as editor, commenter, or viewer"));
        }
        let email = p.email.trim().to_lowercase();
        if email.len() > 254 || email.parse::<lettre::Address>().is_err() {
            return Err(invalid("Enter a valid email address"));
        }
        let domain = email.rsplit_once('@').map(|(_, d)| d).unwrap_or("");
        if !self.store.policy().await?.external_invites
            && !internal_domains
                .iter()
                .any(|d| d.eq_ignore_ascii_case(domain))
        {
            return Err(forbidden(
                "External email invitations are disabled by your organization",
            ));
        }
        let token = random_token();
        let invite_id = new_id();
        let expires_at = timestamp() + 7 * 86400;
        let _writer = self.store.write_lock.lock().await;
        let mut db = self.store.db.clone();
        let mut tx = db
            .transaction_builder()
            .mode(TransactionMode::Immediate)
            .begin()
            .await?;
        authorize_write(&mut tx, &self.user, id, WritePermission::Manage).await?;
        let policy = policy_in(&mut tx).await?;
        if !policy.external_invites
            && !internal_domains
                .iter()
                .any(|d| d.eq_ignore_ascii_case(domain))
        {
            return Err(forbidden(
                "External email invitations are disabled by your organization",
            ));
        }
        toasty::create!(InvitationRecord {
            id: invite_id.clone(),
            token_hash: crate::security::digest(&token),
            canvas_id: id,
            email: email.clone(),
            role: role_string(&p.role),
            expires_at,
            delivered: false,
            accepted_by: "",
            revoked: false
        })
        .exec(&mut tx)
        .await?;
        let canvas = CanvasRecord::filter_by_id(id).get(&mut tx).await?;
        let title = serde_json::from_str::<Canvas>(&canvas.document)?.title;
        tx.commit().await?;
        Ok((
            InvitationSummary {
                id: invite_id,
                email,
                role: p.role,
                expires_at,
                delivered: false,
                accepted: false,
                revoked: false,
            },
            token,
            title,
        ))
    }
    pub async fn invitation_delivered(
        &self,
        id: &str,
        invitation_id: &str,
        success: bool,
    ) -> Result<()> {
        if !self.role(id).await?.manage() {
            return Err(forbidden("Only canvas owners can manage invitations"));
        }
        let _writer = self.store.write_lock.lock().await;
        let mut db = self.store.db.clone();
        let mut tx = db
            .transaction_builder()
            .mode(TransactionMode::Immediate)
            .begin()
            .await?;
        authorize_write(&mut tx, &self.user, id, WritePermission::Manage).await?;
        let mut i = InvitationRecord::filter_by_id(invitation_id)
            .first()
            .exec(&mut tx)
            .await?
            .ok_or_else(|| CanvasError::NotFound(invitation_id.into()))?;
        if i.canvas_id != id {
            return Err(forbidden("Invitation belongs to a different canvas"));
        }
        toasty::update!(i {
            delivered: i.delivered || success,
            revoked: i.revoked || !success
        })
        .exec(&mut tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn accept_invitation(&self, token: &str) -> Result<String> {
        let policy = self.store.policy().await?;
        if !policy.canvas_enabled {
            return Err(forbidden("The canvas feature is disabled"));
        }
        if !self.user.internal && !policy.external_invites {
            return Err(forbidden("External invitations are disabled"));
        }
        let _writer = self.store.write_lock.lock().await;
        let mut db = self.store.db.clone();
        let mut tx = db
            .transaction_builder()
            .mode(TransactionMode::Immediate)
            .begin()
            .await?;
        let policy = policy_in(&mut tx).await?;
        if !policy.canvas_enabled || (!self.user.internal && !policy.external_invites) {
            return Err(forbidden("Canvas or external invitations are disabled"));
        }
        let mut i = InvitationRecord::filter_by_token_hash(crate::security::digest(token))
            .first()
            .exec(&mut tx)
            .await?
            .ok_or_else(|| forbidden("Invitation is invalid or expired"))?;
        if !i.delivered
            || i.revoked
            || i.expires_at <= timestamp()
            || !i.email.eq_ignore_ascii_case(&self.user.email)
            || (!i.accepted_by.is_empty() && i.accepted_by != self.user.id)
        {
            return Err(forbidden(
                "Invitation is invalid, expired, or belongs to a different verified email",
            ));
        }
        let key = format!("{}:{}", i.canvas_id, self.user.id);
        if !i.accepted_by.is_empty() {
            let still_granted = GrantRecord::filter_by_id(&key)
                .first()
                .exec(&mut tx)
                .await?
                .is_some();
            if !still_granted {
                return Err(forbidden(
                    "This invitation has already been used and access was revoked",
                ));
            }
            let id = i.canvas_id.clone();
            tx.commit().await?;
            return Ok(id);
        }

        if let Some(mut existing) = GrantRecord::filter_by_id(&key)
            .first()
            .exec(&mut tx)
            .await?
        {
            if !parse_role(&existing.role)?.manage() {
                toasty::update!(existing {
                    role: i.role.clone()
                })
                .exec(&mut tx)
                .await?;
            }
        } else {
            toasty::create!(GrantRecord {
                id: key,
                canvas_id: i.canvas_id.clone(),
                subject: self.user.id.clone(),
                role: i.role.clone()
            })
            .exec(&mut tx)
            .await?;
        }
        toasty::update!(i {
            accepted_by: self.user.id.clone()
        })
        .exec(&mut tx)
        .await?;
        let id = i.canvas_id.clone();
        tx.commit().await?;
        Ok(id)
    }
    pub async fn comments(&self, id: &str) -> Result<Vec<CommentThread>> {
        self.role(id).await?;
        let records = CommentRecord::filter_by_canvas_id(id)
            .exec(&mut self.store.db.clone())
            .await?;
        let mut threads = records
            .into_iter()
            .map(|r| Ok(serde_json::from_str::<CommentThread>(&r.document)?))
            .collect::<Result<Vec<_>>>()?;
        threads.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(threads)
    }
    pub async fn new_thread(&self, id: &str, p: NewThread) -> Result<CommentThread> {
        if !self.role(id).await?.comment() {
            return Err(forbidden("Your canvas role does not permit comments"));
        }
        check_body(&p.body)?;
        if ![p.x, p.y]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= 100000.)
        {
            return Err(invalid("Invalid comment position"));
        }
        let canvas = self.store.get(id).await?;
        let anchor = if p.anchor_id.is_empty() {
            None
        } else {
            Some(
                canvas
                    .nodes
                    .iter()
                    .find(|n| n.id == p.anchor_id)
                    .ok_or_else(|| CanvasError::NotFound(p.anchor_id.clone()))?,
            )
        };
        let time = now();
        let thread = CommentThread {
            id: new_id(),
            canvas_id: id.into(),
            anchor_id: p.anchor_id,
            anchor_title: anchor.map(|n| n.title.clone()).unwrap_or_default(),
            x: anchor.map(|n| n.x + n.width).unwrap_or(p.x),
            y: anchor.map(|n| n.y).unwrap_or(p.y),
            replies: vec![self.comment(p.body)],
            resolved: false,
            created_at: time.clone(),
            updated_at: time,
        };
        self.store
            .save_thread(id, &thread, true, &self.user)
            .await?;
        Ok(thread)
    }
    pub async fn reply(&self, id: &str, thread_id: &str, p: Reply) -> Result<CommentThread> {
        if !self.role(id).await?.comment() {
            return Err(forbidden("Your canvas role does not permit comments"));
        }
        check_body(&p.body)?;
        self.store
            .change_thread(id, thread_id, &self.user, |thread, _| {
                if thread.replies.len() >= 500 {
                    return Err(invalid("Comment thread limit reached"));
                }
                thread.replies.push(self.comment(p.body));
                Ok(())
            })
            .await
    }
    pub async fn resolve(
        &self,
        id: &str,
        thread_id: &str,
        p: ResolveThread,
    ) -> Result<CommentThread> {
        let role = self.role(id).await?;
        if !role.comment() {
            return Err(forbidden("Your canvas role does not permit comments"));
        }
        self.store
            .change_thread(id, thread_id, &self.user, |thread, current_role| {
                if !current_role.manage()
                    && thread
                        .replies
                        .first()
                        .is_none_or(|c| c.author_id != self.user.id)
                {
                    return Err(forbidden(
                        "Only the thread author or a canvas owner can resolve or reopen it",
                    ));
                }
                thread.resolved = p.resolved;
                Ok(())
            })
            .await
    }
    fn comment(&self, body: String) -> Comment {
        Comment {
            id: new_id(),
            author_id: self.user.id.clone(),
            author_name: self.user.name.clone(),
            body,
            created_at: now(),
        }
    }
    pub async fn set_policy(&self, p: OrganizationPolicy) -> Result<OrganizationPolicy> {
        if !self.user.admin {
            return Err(forbidden("Organization administrator access is required"));
        }
        self.store.set_policy(p).await
    }
}
fn check_body(body: &str) -> Result<()> {
    if body.trim().is_empty() || body.len() > 20000 {
        Err(invalid("Comments must contain 1–20,000 bytes"))
    } else {
        Ok(())
    }
}

impl Store {
    pub async fn policy(&self) -> Result<OrganizationPolicy> {
        Ok(Metadata::filter_by_id("organization_policy")
            .first()
            .exec(&mut self.db.clone())
            .await?
            .map(|m| serde_json::from_str(&m.value))
            .transpose()?
            .unwrap_or_default())
    }
    async fn set_policy(&self, p: OrganizationPolicy) -> Result<OrganizationPolicy> {
        let _writer = self.write_lock.lock().await;
        let mut db = self.db.clone();
        if let Some(mut record) = Metadata::filter_by_id("organization_policy")
            .first()
            .exec(&mut db)
            .await?
        {
            toasty::update!(record {
                value: serde_json::to_string(&p)?
            })
            .exec(&mut db)
            .await?;
        } else {
            toasty::create!(Metadata {
                id: "organization_policy",
                value: serde_json::to_string(&p)?
            })
            .exec(&mut db)
            .await?;
        }
        Ok(p)
    }
    async fn save_thread(
        &self,
        id: &str,
        thread: &CommentThread,
        new: bool,
        user: &UserInfo,
    ) -> Result<()> {
        let _writer = self.write_lock.lock().await;
        let mut db = self.db.clone();
        let mut tx = db
            .transaction_builder()
            .mode(TransactionMode::Immediate)
            .begin()
            .await?;
        authorize_write(&mut tx, user, id, WritePermission::Comment).await?;
        if !thread.anchor_id.is_empty() {
            let record = CanvasRecord::filter_by_id(id).get(&mut tx).await?;
            let canvas: Canvas = serde_json::from_str(&record.document)?;
            if !canvas.nodes.iter().any(|n| n.id == thread.anchor_id) {
                return Err(invalid(
                    "The comment anchor was deleted. Select a new canvas position and retry.",
                ));
            }
        }
        if new {
            toasty::create!(CommentRecord {
                id: thread.id.clone(),
                canvas_id: id,
                document: serde_json::to_string(thread)?
            })
            .exec(&mut tx)
            .await?;
        } else {
            let mut record = CommentRecord::filter_by_id(&thread.id).get(&mut tx).await?;
            toasty::update!(record {
                document: serde_json::to_string(thread)?
            })
            .exec(&mut tx)
            .await?;
        }
        let mut record = CanvasRecord::filter_by_id(id).get(&mut tx).await?;
        let mut canvas: Canvas = serde_json::from_str(&record.document)?;
        canvas.revision += 1;
        canvas.updated_at = now();
        toasty::update!(record {
            document: serde_json::to_string(&canvas)?,
            updated_at: canvas.updated_at.clone()
        })
        .exec(&mut tx)
        .await?;
        toasty::create!(MutationRecord{id:format!("{}:{}",id,canvas.revision),canvas_id:id,revision:canvas.revision as i64,body:serde_json::json!({"comment_thread":thread.id,"actor":thread.replies.last().map(|c|c.author_id.as_str())}).to_string(),created_at:canvas.updated_at.clone()}).exec(&mut tx).await?;
        tx.commit().await?;
        let _ = self.changes.send(Change {
            canvas_id: id.into(),
            revision: canvas.revision,
        });
        Ok(())
    }
    async fn change_thread(
        &self,
        id: &str,
        thread_id: &str,
        user: &UserInfo,
        change: impl FnOnce(&mut CommentThread, AccessRole) -> Result<()>,
    ) -> Result<CommentThread> {
        // Serialize edits through the same gate as canvas mutations. Re-read inside
        // the gate so replies are appended rather than replacing another reply.
        let _writer = self.write_lock.lock().await;
        let mut db = self.db.clone();
        let mut tx = db
            .transaction_builder()
            .mode(TransactionMode::Immediate)
            .begin()
            .await?;
        let current_role = authorize_write(&mut tx, user, id, WritePermission::Comment).await?;
        let mut record = CommentRecord::filter_by_id(thread_id)
            .first()
            .exec(&mut tx)
            .await?
            .ok_or_else(|| CanvasError::NotFound(thread_id.into()))?;
        if record.canvas_id != id {
            return Err(forbidden("Thread belongs to a different canvas"));
        }
        let mut thread: CommentThread = serde_json::from_str(&record.document)?;
        change(&mut thread, current_role)?;
        thread.updated_at = now();
        toasty::update!(record {
            document: serde_json::to_string(&thread)?
        })
        .exec(&mut tx)
        .await?;
        let mut c = CanvasRecord::filter_by_id(id).get(&mut tx).await?;
        let mut canvas: Canvas = serde_json::from_str(&c.document)?;
        canvas.revision += 1;
        canvas.updated_at = now();
        toasty::update!(c {
            document: serde_json::to_string(&canvas)?,
            updated_at: canvas.updated_at.clone()
        })
        .exec(&mut tx)
        .await?;
        toasty::create!(MutationRecord {
            id: format!("{}:{}", id, canvas.revision),
            canvas_id: id,
            revision: canvas.revision as i64,
            body: serde_json::json!({"comment_thread":thread.id}).to_string(),
            created_at: canvas.updated_at.clone()
        })
        .exec(&mut tx)
        .await?;
        tx.commit().await?;
        let _ = self.changes.send(Change {
            canvas_id: id.into(),
            revision: canvas.revision,
        });
        Ok(thread)
    }
}

#[cfg(test)]
#[path = "collab_tests.rs"]
mod tests;
