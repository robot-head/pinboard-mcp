use crate::{browser, state::*};
use gloo_net::http::Request;
use leptos::{prelude::*, task::spawn_local};
use pinboard_core::*;
use serde_json::{Value, json};
impl App {
    pub fn can_edit(self) -> bool {
        self.role.get().edit() && self.me.get().is_none_or(|m| m.policy.canvas_enabled)
    }
    pub fn can_comment(self) -> bool {
        self.role.get().comment()
    }
    pub fn load_me(self) {
        spawn_local(async move {
            if let Ok(me) = get::<Me>("/api/me").await {
                if me.user.is_none() || !me.policy.canvas_enabled {
                    browser::stop_live();
                    self.board.set(None);
                    self.library.set(vec![]);
                    self.following.set(String::new());
                }
                if me.user.is_some() && me.policy.canvas_enabled {
                    let id = self.active.get_untracked();
                    if !id.is_empty() {
                        if let Ok(value) =
                            get::<serde_json::Value>(&format!("/api/canvases/{id}/access")).await
                        {
                            if let Ok(role) = serde_json::from_value(value["role"].clone()) {
                                self.role.set(role);
                            }
                        } else {
                            browser::stop_live();
                            self.board.set(None);
                            self.threads.set(vec![]);
                            self.active.set(String::new());
                            if let Ok(list) = get::<Vec<CanvasSummary>>("/api/canvases").await {
                                self.library.set(list);
                            }
                            self.status
                                .set("Access to this canvas is no longer available".into());
                        }
                    }
                }
                self.me.set(Some(me));
            }
        });
    }
    pub fn load_threads(self) {
        let id = self.active.get_untracked();
        if id.is_empty() {
            return;
        }
        spawn_local(async move {
            if let Ok(threads) =
                get::<Vec<CommentThread>>(&format!("/api/canvases/{id}/comments")).await
                && self.active.get_untracked() == id
            {
                self.threads.set(threads);
            }
        });
    }
    pub fn new_thread(self, body: String, draft: RwSignal<String>) {
        let p = self.center();
        let selected = self.selected();
        let submitted = body.clone();
        let request = NewThread {
            anchor_id: selected.as_ref().map(|n| n.id.clone()).unwrap_or_default(),
            x: p.x,
            y: p.y,
            body,
        };
        let id = self.active.get_untracked();
        spawn_local(async move {
            match post::<CommentThread>(&format!("/api/canvases/{id}/comments"), &request).await {
                Ok(thread) => {
                    if draft.get_untracked() == submitted {
                        draft.set(String::new());
                    }
                    self.active_thread.set(thread.id);
                    self.refresh();
                    self.status.set("Comment added".into());
                }
                Err(e) => self.status.set(e),
            }
        });
    }
    pub fn reply_thread(self, thread: String, body: String, draft: RwSignal<String>) {
        let id = self.active.get_untracked();
        let submitted = body.clone();
        spawn_local(async move {
            match post::<CommentThread>(
                &format!("/api/canvases/{id}/comments/{thread}/replies"),
                &Reply { body },
            )
            .await
            {
                Ok(_) => {
                    self.refresh();
                    if draft.get_untracked() == submitted {
                        draft.set(String::new());
                    }
                    self.status.set("Reply added".into());
                }
                Err(e) => self.status.set(e),
            }
        });
    }
    pub fn resolve_thread(self, thread: String, resolved: bool) {
        let id = self.active.get_untracked();
        spawn_local(async move {
            match write(
                "PUT",
                &format!("/api/canvases/{id}/comments/{thread}"),
                json!({"resolved":resolved}),
            )
            .await
            {
                Ok(_) => {
                    self.refresh();
                    self.status.set(
                        if resolved {
                            "Thread resolved"
                        } else {
                            "Thread reopened"
                        }
                        .into(),
                    );
                }
                Err(e) => self.status.set(e),
            }
        });
    }
    pub fn open_sharing(self) {
        self.share_open.set(true);
        self.sharing.set(None);
        self.reload_sharing();
    }
    pub fn reload_sharing(self) {
        let id = self.active.get_untracked();
        spawn_local(async move {
            match get::<Sharing>(&format!("/api/canvases/{id}/sharing")).await {
                Ok(sharing) => self.sharing.set(Some(sharing)),
                Err(e) => self.status.set(e),
            }
        });
    }
    pub fn invite(self, email: String, role: AccessRole) {
        let id = self.active.get_untracked();
        spawn_local(async move {
            self.status.set("Sending invitation…".into());
            match post::<InvitationSummary>(
                &format!("/api/canvases/{id}/invitations"),
                &InviteRequest { email, role },
            )
            .await
            {
                Ok(_) => {
                    self.status.set("Invitation email sent".into());
                    self.reload_sharing();
                }
                Err(e) => self.status.set(e),
            }
        });
    }
    pub fn revoke_member(self, subject: String) {
        let id = self.active.get_untracked();
        spawn_local(async move {
            match write(
                "DELETE",
                &format!("/api/canvases/{id}/members/{subject}"),
                Value::Null,
            )
            .await
            {
                Ok(_) => {
                    self.reload_sharing();
                    self.status.set("Access revoked".into());
                }
                Err(e) => self.status.set(e),
            }
        });
    }
    pub fn revoke_invite(self, invitation: String) {
        let id = self.active.get_untracked();
        spawn_local(async move {
            match write(
                "DELETE",
                &format!("/api/canvases/{id}/invitations/{invitation}"),
                Value::Null,
            )
            .await
            {
                Ok(_) => {
                    self.reload_sharing();
                    self.status.set("Invitation revoked".into());
                }
                Err(e) => self.status.set(e),
            }
        });
    }
    pub fn save_policy(self, policy: OrganizationPolicy) {
        spawn_local(async move {
            match write(
                "PUT",
                "/api/settings/artifacts",
                serde_json::to_value(&policy).unwrap(),
            )
            .await
            {
                Ok(_) => {
                    self.me.update(|me| {
                        if let Some(me) = me {
                            me.policy = policy.clone();
                        }
                    });
                    if policy.canvas_enabled {
                        self.start();
                    } else {
                        browser::stop_live();
                        self.board.set(None);
                    }
                    self.status.set("Organization policy saved".into());
                }
                Err(e) => self.status.set(e),
            }
        });
    }
}
pub async fn write(method: &str, url: &str, body: Value) -> Result<Value, String> {
    let request = if method == "DELETE" {
        Request::delete(url)
    } else {
        Request::put(url)
    };
    let response = request
        .json(&body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|_| "Connection lost; the operation was not confirmed".to_string())?;
    if response.status() == 204 {
        Ok(Value::Null)
    } else {
        decode(response).await
    }
}
