use crate::{Icon, browser, client_collab::write, state::*};
use leptos::{prelude::*, task::spawn_local};
use pinboard_core::*;

fn dialog() {
    browser::start_dialog();
    on_cleanup(browser::end_dialog);
}
#[component]
pub fn LoginScreen() -> impl IntoView {
    let app = expect_context::<App>();
    let suffix = || {
        let path = web_sys::window()
            .map(|w| {
                format!(
                    "{}{}",
                    w.location().pathname().unwrap_or_default(),
                    w.location().search().unwrap_or_default()
                )
            })
            .unwrap_or_else(|| "/".into());
        format!("?return_to={}", js_sys::encode_uri_component(&path))
    };
    view! {<main class="login-screen"><div class="brand"><span class="brand-mark"><i></i><i></i><i></i><i></i></span>"pinboard"</div><h1>"A space for your next good idea."</h1><p>"Sign in with your organization to open your canvases and shared work."</p>{move||app.me.get().map(|me|{let multiple=me.providers.len()>1;me.providers.into_iter().map(|p|view!{<a class="button primary login-provider" href=format!("{}{}",p.login_url,suffix())>{if multiple{format!("Sign in with {}",p.label)}else{"Sign in".into()}}<Icon name="external"/></a>}).collect_view()})}<Show when=move||app.me.get().is_none()><p>"Opening your workspace…"</p></Show><p class="muted small" role="status">{move||app.status.get()}</p></main>}
}

#[component]
pub fn CommentPins() -> impl IntoView {
    let app = expect_context::<App>();
    view! {<Show when=move||app.comment_mode.get()><For each={move||app.threads.get().into_iter().filter(|t|!t.resolved).collect::<Vec<_>>()} key=|t|t.id.clone() children=move|thread|{let id=StoredValue::new(thread.id);let label=thread.replies.first().map(|c|format!("Comment by {}: {}",c.author_name,c.body.chars().take(80).collect::<String>())).unwrap_or_else(||"Comment thread".into());view!{<button id=move||format!("pin-{}",id.get_value()) class="comment-pin" aria-label=label style=move||{let current=app.threads.get().into_iter().find(|t|t.id==id.get_value());current.map(|t|{let anchor=app.board.get().and_then(|c|c.nodes.into_iter().find(|n|n.id==t.anchor_id)).map(|n|app.node_display(n));let(x,y)=anchor.map(|n|(n.x+n.width,n.y)).unwrap_or((t.x,t.y));format!("left:{x}px;top:{y}px")}).unwrap_or_default()} on:pointerdown=|e|e.stop_propagation() on:click=move|_|{app.active_thread.set(id.get_value());app.panel.set(Panel::Comments);browser::focus_element(&format!("thread-{}",id.get_value()));}><Icon name="comment"/></button>}}/></Show>}
}

#[component]
pub fn CommentsPanel() -> impl IntoView {
    let app = expect_context::<App>();
    let body = RwSignal::new(String::new());
    let show_resolved = RwSignal::new(false);
    view! {<div class="comments-panel"><span class="eyebrow">"KEEP THE CONVERSATION CLOSE"</span><h2>"A little context, right here."</h2><p class="muted small">"Select an idea in Comment mode to anchor feedback to it. Deleted anchors leave their discussion in place."</p><label class="checkbox-label"><input type="checkbox" prop:checked=move||show_resolved.get() on:change=move|e|show_resolved.set(event_target_checked(&e))/>"Show resolved threads"</label>
    <div class="threads" aria-label="Comment threads"><For each={move||app.threads.get().into_iter().filter(|t|show_resolved.get()||!t.resolved).collect::<Vec<_>>()} key=|t|t.id.clone() children=move|t|view!{<Thread id=t.id/>}/></div>
    <div class="panel-divider"></div><div class="form"><label for="new-comment">{move||app.selected().map(|n|format!("Comment on {}",n.title)).unwrap_or_else(||"Comment at the center of your view".into())}</label><textarea id="new-comment" rows="4" maxlength="20000" placeholder="What would you like to add?" prop:value=move||body.get() on:input=move|e|body.set(event_target_value(&e)) disabled=move||!app.can_comment()></textarea><button class="button primary" disabled=move||body.get().trim().is_empty()||!app.can_comment() on:click=move|_|{app.new_thread(body.get_untracked(),body);}>"Add comment"</button></div></div>}
}

#[component]
fn Thread(id: String) -> impl IntoView {
    let app = expect_context::<App>();
    let key = StoredValue::new(id);
    let reply = RwSignal::new(String::new());
    view! {{move||app.threads.get().into_iter().find(|t|t.id==key.get_value()).map(|t|{let resolved=t.resolved;let own=app.me.get().and_then(|m|m.user).is_some_and(|u|t.replies.first().is_some_and(|c|c.author_id==u.id));view!{<article data-thread-id=key.get_value() class="comment-thread" class:active=app.active_thread.get()==key.get_value()><button id=format!("thread-{}",t.id) class="thread-heading" on:click=move|_|app.active_thread.set(key.get_value())>{if t.anchor_id.is_empty()&&!t.anchor_title.is_empty(){format!("{} · deleted anchor",t.anchor_title)}else if t.anchor_title.is_empty(){"Canvas discussion".into()}else{t.anchor_title.clone()}}</button><Show when=move||resolved><span class="resolved-label">"Resolved"</span></Show><div class="replies">{t.replies.into_iter().map(|c|view!{<div class="comment-reply"><strong>{c.author_name}</strong><time datetime=c.created_at.clone()>{c.created_at.chars().take(10).collect::<String>()}</time><p>{c.body}</p></div>}).collect_view()}</div><Show when=move||app.active_thread.get()==key.get_value()><div class="form"><label for=format!("reply-{}",key.get_value())>"Reply"</label><textarea id=format!("reply-{}",key.get_value()) rows="2" maxlength="20000" prop:value=move||reply.get() on:input=move|e|reply.set(event_target_value(&e)) disabled=move||!app.can_comment()></textarea><button class="button" disabled=move||reply.get().trim().is_empty()||!app.can_comment() on:click=move|_|{app.reply_thread(key.get_value(),reply.get_untracked(),reply);}>"Post reply"</button></div></Show><Show when=move||app.can_comment()&&(own||app.role.get().manage())><button class="resolve-thread" on:click=move|_|app.resolve_thread(key.get_value(),!resolved)>{if resolved{"Reopen thread"}else{"Resolve thread"}}</button></Show></article>}})}}
}

#[component]
pub fn ShareModal() -> impl IntoView {
    let app = expect_context::<App>();
    dialog();
    let email = RwSignal::new(String::new());
    let role = RwSignal::new("editor".to_string());
    view! {<div class="modal-backdrop" on:click=move|_|app.share_open.set(false)><section class="setup-modal sharing-modal" role="dialog" aria-modal="true" aria-labelledby="share-title" on:click=|e|e.stop_propagation()><button class="modal-close icon-button" aria-label="Close sharing" on:click=move|_|app.share_open.set(false)><Icon name="close"/></button><span class="eyebrow">"BRING PEOPLE INTO THE PICTURE"</span><h2 id="share-title">"Share a little thinking space."</h2><p>"Invitations are tied to the recipient’s verified sign-in email and expire after seven days."</p>
    <Show when=move||app.me.get().is_some_and(|m|m.auth_mode!="local"&&m.mail_available) fallback=||view!{<p class="service-notice">"Team invitations become available when organization sign-in and email delivery are configured."</p>}><div class="form"><label for="invite-email">"Email address"</label><input id="invite-email" type="email" placeholder="teammate@example.com" prop:value=move||email.get() on:input=move|e|email.set(event_target_value(&e))/><label for="invite-role">"Access"</label><select id="invite-role" on:change=move|e|role.set(event_target_value(&e))><option value="editor">"Can edit and comment"</option><option value="commenter">"Can comment"</option><option value="viewer">"Can view"</option></select><button class="button primary" disabled=move||email.get().trim().is_empty()||app.sharing.get().is_none() on:click=move|_|{app.invite(email.get_untracked(),if role.get_untracked()=="viewer"{AccessRole::Viewer}else if role.get_untracked()=="commenter"{AccessRole::Commenter}else{AccessRole::Editor});email.set(String::new());}>"Send invitation"</button></div></Show>
    <div class="panel-divider"></div><h3>"People with access"</h3>{move||app.sharing.get().map(|sharing|view!{<div class="members-list">{sharing.members.into_iter().map(|member|{let subject=StoredValue::new(member.user_id);let owner=member.role.manage();view!{<div class="member"><div><strong>{member.name}</strong><small>{member.email}</small></div><span>{role_label(&member.role)}</span>{(!owner).then(||view!{<button class="icon-button" aria-label="Revoke member access" on:click=move|_|app.revoke_member(subject.get_value())><Icon name="close"/></button>})}</div>}}).collect_view()}</div>})}
    <h3 class="pending-heading">"Invitations"</h3>{move||app.sharing.get().map(|sharing|view!{<div>{sharing.invitations.into_iter().map(|invitation|{let id=StoredValue::new(invitation.id);let pending=!invitation.revoked&&!invitation.accepted;view!{<div class="member"><div><strong>{invitation.email}</strong><small>{if invitation.revoked{"Revoked"}else if invitation.accepted{"Accepted"}else if invitation.delivered{"Email sent"}else{"Delivery pending"}}</small></div><span>{role_label(&invitation.role)}</span>{pending.then(||view!{<button class="icon-button" aria-label="Revoke invitation" on:click=move|_|app.revoke_invite(id.get_value())><Icon name="close"/></button>})}</div>}}).collect_view()}</div>})}
    <p class="small muted" role="status">{move||app.status.get()}</p></section></div>}
}
fn role_label(role: &AccessRole) -> &'static str {
    match role {
        AccessRole::Owner => "Owner",
        AccessRole::Editor => "Editor",
        AccessRole::Commenter => "Commenter",
        AccessRole::Viewer => "Viewer",
    }
}

#[component]
pub fn SettingsModal() -> impl IntoView {
    let app = expect_context::<App>();
    dialog();
    let enabled = RwSignal::new(
        app.me
            .get_untracked()
            .is_none_or(|m| m.policy.canvas_enabled),
    );
    let external = RwSignal::new(
        app.me
            .get_untracked()
            .is_some_and(|m| m.policy.external_invites),
    );
    let token = RwSignal::new(String::new());
    let tokens = RwSignal::new(Vec::<serde_json::Value>::new());
    let load = move || {
        spawn_local(async move {
            if let Ok(value) = get::<Vec<serde_json::Value>>("/api/tokens").await {
                tokens.set(value);
            }
        })
    };
    load();
    view! {<div class="modal-backdrop" on:click=move|_|app.settings_open.set(false)><section class="setup-modal" role="dialog" aria-modal="true" aria-labelledby="settings-title" on:click=|e|e.stop_propagation()><button class="modal-close icon-button" aria-label="Close settings" on:click=move|_|app.settings_open.set(false)><Icon name="close"/></button><span class="eyebrow">"YOUR WORKSPACE"</span><h2 id="settings-title">"Account & organization settings"</h2><p>{move||app.me.get().and_then(|m|m.user).map(|u|format!("{} · {}",u.name,u.email)).unwrap_or_default()}</p><Show when=move||app.me.get().and_then(|m|m.user).is_some_and(|u|u.admin)><h3>"Organization settings → Artifacts"</h3><div class="policy-controls"><label class="checkbox-label"><input type="checkbox" prop:checked=move||enabled.get() on:change=move|e|enabled.set(event_target_checked(&e))/>"Enable visual canvas"</label><label class="checkbox-label"><input type="checkbox" prop:checked=move||external.get() on:change=move|e|external.set(event_target_checked(&e))/>"Permit external email invitations"</label><button class="button primary" on:click=move|_|app.save_policy(OrganizationPolicy{canvas_enabled:enabled.get_untracked(),external_invites:external.get_untracked()})>"Save organization policy"</button></div></Show>
    <Show when=move||app.me.get().is_some_and(|m|m.auth_mode!="local")><div class="panel-divider"></div><h3>"MCP access tokens"</h3><p class="small muted">"Tokens use your canvas permissions, expire after 30 days, and can be revoked. The token is shown once."</p><button class="button" on:click=move|_|spawn_local(async move{match post::<serde_json::Value>("/api/tokens",&serde_json::json!({})).await{Ok(result)=>{token.set(result["token"].as_str().unwrap_or_default().into());load();},Err(e)=>app.status.set(e)}})>"Create MCP access token"</button><Show when=move||!token.get().is_empty()><div class="form"><label for="new-token">"Copy this token before closing"</label><textarea id="new-token" readonly prop:value=move||token.get()></textarea><button class="button" on:click=move|_|app.copy(token.get_untracked())>"Copy token"</button></div></Show><div class="token-list">{move||tokens.get().into_iter().map(|entry|{let id=StoredValue::new(entry["id"].as_str().unwrap_or_default().to_string());view!{<div class="member"><span>{format!("{}…",id.get_value().chars().take(10).collect::<String>())}</span><button class="button" on:click=move|_|spawn_local(async move{match write("DELETE",&format!("/api/tokens/{}",id.get_value()),serde_json::Value::Null).await{Ok(_)=>load(),Err(e)=>app.status.set(e)}})>"Revoke token"</button></div>}}).collect_view()}</div><form method="post" action="/auth/logout"><button class="button">"Sign out"</button></form></Show><p class="small muted" role="status">{move||app.status.get()}</p></section></div>}
}
