use crate::{
    collab::Workspace,
    mail::Mailer,
    security::{Actor, Security},
};
use crate::{
    export,
    mcp::{CanvasMcp, Create},
    store::{Store, StoreError},
};
use axum::{
    Extension, Json, Router,
    extract::{
        Path, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, delete, get, post, put},
};
use futures_util::{SinkExt, StreamExt};
use pinboard_core::*;
use rmcp::transport::streamable_http_server::{
    StreamableHttpService, session::local::LocalSessionManager,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{RwLock, broadcast};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Presence {
    pub id: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub security: Security,
    mailer: Mailer,
    mcp: Arc<tokio::sync::Mutex<HashMap<String, StreamableHttpService<CanvasMcp>>>>,
    peers: Arc<RwLock<HashMap<String, HashMap<String, Presence>>>>,
    presence: broadcast::Sender<String>,
}
impl AppState {
    pub fn new(store: Store, security: Security, mailer: Mailer) -> Self {
        let (presence, _) = broadcast::channel(256);
        Self {
            store,
            security,
            mailer,
            mcp: Default::default(),
            peers: Arc::default(),
            presence,
        }
    }
}

impl IntoResponse for StoreError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            StoreError::Canvas(CanvasError::Conflict { expected, actual }) => (
                StatusCode::CONFLICT,
                CanvasError::Conflict { expected, actual }.to_string(),
            ),
            StoreError::Canvas(CanvasError::NotFound(id)) => {
                (StatusCode::NOT_FOUND, format!("Not found: {id}"))
            }
            StoreError::Forbidden(message) => (StatusCode::FORBIDDEN, message),
            StoreError::Unauthorized => (StatusCode::UNAUTHORIZED, "Sign in is required".into()),
            StoreError::Unavailable(message) => (StatusCode::SERVICE_UNAVAILABLE, message),
            StoreError::Canvas(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            StoreError::Internal(e) => {
                tracing::error!(error=%e,"Storage operation failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Storage operation failed".into(),
                )
            }
        };
        (status, Json(serde_json::json!({"error":message}))).into_response()
    }
}

async fn me(
    State(s): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Me>, StoreError> {
    let user = match s.security.authenticate(&headers).await {
        Ok(actor) => Some(actor.user),
        Err(StoreError::Unauthorized) => None,
        Err(e) => return Err(e),
    };
    Ok(Json(Me {
        user,
        auth_mode: if s.security.configured() {
            "sso"
        } else {
            "local"
        }
        .into(),
        policy: s.store.policy().await?,
        mail_available: s.mailer.available(),
        login_url: s
            .security
            .providers()
            .first()
            .map(|p| p.login_url.clone())
            .unwrap_or_else(|| "/auth/login".into()),
        providers: s.security.providers(),
    }))
}
async fn tokens(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
) -> Result<Json<Vec<crate::security::TokenInfo>>, StoreError> {
    Ok(Json(s.security.tokens(&actor.user).await?))
}
async fn create_token(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
) -> Result<Json<serde_json::Value>, StoreError> {
    let (token, info) = s.security.api_token(&actor.user).await?;
    Ok(Json(serde_json::json!({"token":token,"info":info})))
}
async fn revoke_token(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> Result<StatusCode, StoreError> {
    s.security.revoke(&actor.user, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn policy(State(s): State<AppState>) -> Result<Json<OrganizationPolicy>, StoreError> {
    Ok(Json(s.store.policy().await?))
}
async fn set_policy(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(p): Json<OrganizationPolicy>,
) -> Result<Json<OrganizationPolicy>, StoreError> {
    Ok(Json(workspace(&s, actor).set_policy(p).await?))
}
async fn access(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StoreError> {
    Ok(Json(
        serde_json::json!({"role":workspace(&s,actor).role(&id).await?}),
    ))
}
async fn sharing(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> Result<Json<Sharing>, StoreError> {
    Ok(Json(workspace(&s, actor).sharing(&id).await?))
}
async fn invite(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(p): Json<InviteRequest>,
) -> Result<Json<InvitationSummary>, StoreError> {
    if !s.security.configured() {
        return Err(StoreError::Unavailable(
            "Configure organization sign-in before inviting collaborators".into(),
        ));
    }
    if !s.mailer.available() {
        return Err(StoreError::Unavailable(
            "Email invitations are unavailable until SMTP is configured".into(),
        ));
    }
    let w = workspace(&s, actor);
    let (mut summary, token, title) = w
        .prepare_invitation(&id, p, &s.security.internal_domains())
        .await?;
    let link = format!("{}/?invite={token}", s.security.base_url());
    let sent = s
        .mailer
        .invitation(&summary.email, &title, &w.user.name, &link)
        .await;
    if let Err(error) = sent {
        tracing::error!(error=%error,"Invitation delivery failed");
        w.invitation_delivered(&id, &summary.id, false).await?;
        return Err(StoreError::Unavailable(
            "Invitation email could not be delivered. The invitation was revoked; you can retry."
                .into(),
        ));
    }
    w.invitation_delivered(&id, &summary.id, true).await?;
    summary.delivered = true;
    Ok(Json(summary))
}
async fn revoke_invitation(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((id, invitation)): Path<(String, String)>,
) -> Result<StatusCode, StoreError> {
    workspace(&s, actor)
        .invitation_delivered(&id, &invitation, false)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn revoke_member(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((id, subject)): Path<(String, String)>,
) -> Result<StatusCode, StoreError> {
    workspace(&s, actor).revoke_member(&id, &subject).await?;
    Ok(StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
struct AcceptInvite {
    token: String,
}
async fn accept_invitation(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(p): Json<AcceptInvite>,
) -> Result<Json<serde_json::Value>, StoreError> {
    Ok(Json(
        serde_json::json!({"canvas_id":workspace(&s,actor).accept_invitation(&p.token).await?}),
    ))
}
async fn comments(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> Result<Json<Vec<CommentThread>>, StoreError> {
    Ok(Json(workspace(&s, actor).comments(&id).await?))
}
async fn new_comment(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(p): Json<NewThread>,
) -> Result<Json<CommentThread>, StoreError> {
    Ok(Json(workspace(&s, actor).new_thread(&id, p).await?))
}
async fn reply_comment(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((id, thread)): Path<(String, String)>,
    Json(p): Json<Reply>,
) -> Result<Json<CommentThread>, StoreError> {
    Ok(Json(workspace(&s, actor).reply(&id, &thread, p).await?))
}
async fn resolve_comment(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((id, thread)): Path<(String, String)>,
    Json(p): Json<ResolveThread>,
) -> Result<Json<CommentThread>, StoreError> {
    Ok(Json(workspace(&s, actor).resolve(&id, &thread, p).await?))
}

#[cfg(test)]
pub fn router(store: Store, dist: PathBuf, base_url: String, port: u16) -> Router {
    let security = Security::local(store.clone(), base_url.clone());
    router_configured(store, dist, base_url, port, security, Mailer::default())
}
pub fn router_configured(
    store: Store,
    dist: PathBuf,
    base_url: String,
    port: u16,
    security: Security,
    mailer: Mailer,
) -> Router {
    let state = AppState::new(store, security, mailer);
    let auth_state = state.clone();
    Router::new()
        .route(
            "/api/health",
            get(|| async {
                Json(serde_json::json!({"status":"ok","version":env!("CARGO_PKG_VERSION")}))
            }),
        )
        .route("/api/me", get(me))
        .route("/auth/login", get(crate::security::login))
        .route("/auth/saml/metadata", get(crate::saml::metadata))
        .route("/auth/saml/login", get(crate::saml::login))
        .route("/auth/saml/acs", post(crate::saml::acs))
        .route("/auth/callback", get(crate::security::callback))
        .route("/auth/logout", post(crate::security::logout))
        .route("/api/tokens", get(tokens).post(create_token))
        .route("/api/tokens/{token_id}", delete(revoke_token))
        .route("/api/settings/artifacts", get(policy).put(set_policy))
        .route("/api/invitations/accept", post(accept_invitation))
        .route("/api/canvases", get(list).post(create))
        .route("/api/canvases/{id}", get(read))
        .route("/api/canvases/{id}/access", get(access))
        .route("/api/canvases/{id}/sharing", get(sharing))
        .route("/api/canvases/{id}/invitations", post(invite))
        .route(
            "/api/canvases/{id}/invitations/{invitation}",
            delete(revoke_invitation),
        )
        .route(
            "/api/canvases/{id}/members/{subject}",
            delete(revoke_member),
        )
        .route(
            "/api/canvases/{id}/comments",
            get(comments).post(new_comment),
        )
        .route(
            "/api/canvases/{id}/comments/{thread}/replies",
            post(reply_comment),
        )
        .route("/api/canvases/{id}/comments/{thread}", put(resolve_comment))
        .route("/api/canvases/{id}/mutations", post(edit))
        .route("/api/canvases/{id}/export/{format}", get(download))
        .route("/api/canvases/{id}/live", get(live))
        .route("/mcp", any(mcp_endpoint))
        .fallback_service(
            ServeDir::new(&dist).not_found_service(ServeFile::new(dist.join("index.html"))),
        )
        .with_state(state)
        .layer(axum::extract::DefaultBodyLimit::max(4 * 1024 * 1024))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(
            4 * 1024 * 1024,
        ))
        .layer(middleware::from_fn(move |req: Request, next: Next| {
            authenticate(auth_state.clone(), req, next)
        }))
        .layer(middleware::from_fn(move |req: Request, next: Next| {
            origin_guard(req, next, port, base_url.clone())
        }))
}
async fn authenticate(s: AppState, mut request: Request, next: Next) -> Response {
    let path = request.uri().path();
    if (path.starts_with("/api/") && !matches!(path, "/api/health" | "/api/me")) || path == "/mcp" {
        match s.security.authenticate(request.headers()).await {
            Ok(actor) => {
                if actor.expires_at <= chrono::Utc::now().timestamp() {
                    return StoreError::Unauthorized.into_response();
                }
                request.extensions_mut().insert(actor);
            }
            Err(e) => return e.into_response(),
        }
    }
    next.run(request).await
}
async fn origin_guard(req: Request, next: Next, port: u16, base_url: String) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    let mut allowed = vec![
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
        format!("http://[::1]:{port}"),
    ];
    allowed.push(base_url);
    let host_ok = allowed.iter().any(|o| {
        o.split_once("://")
            .is_some_and(|(_, authority)| authority == host)
    });
    let origin_ok = req
        .headers()
        .get(header::ORIGIN)
        .map(|o| {
            o.to_str()
                .ok()
                .is_some_and(|o| allowed.iter().any(|a| a == o))
        })
        .unwrap_or(true);
    let saml_post =
        req.uri().path() == "/auth/saml/acs" && req.method() == axum::http::Method::POST;
    if !host_ok || (!origin_ok && !saml_post) {
        return (StatusCode::FORBIDDEN, "Host or Origin is not permitted").into_response();
    }
    let private = req.uri().path().starts_with("/api/")
        || req.uri().path().starts_with("/auth/")
        || req.uri().path() == "/mcp";
    let mut response = next.run(req).await;
    response.headers_mut().insert(
        "x-content-type-options",
        header::HeaderValue::from_static("nosniff"),
    );
    response.headers_mut().insert(
        "referrer-policy",
        header::HeaderValue::from_static("no-referrer"),
    );
    if private {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("no-store"),
        );
    }
    response
}
fn workspace(s: &AppState, actor: Actor) -> Workspace {
    Workspace::new(s.store.clone(), actor.user)
}
async fn mcp_endpoint(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    req: Request,
) -> Response {
    // A stable account can return through SSO with a changed email/role/name.
    // Do not reuse a transport factory that captured an older principal.
    let principal = match serde_json::to_string(&actor.user) {
        Ok(value) => crate::security::digest(&value),
        Err(error) => return StoreError::from(error).into_response(),
    };
    let service = {
        let mut services = s.mcp.lock().await;
        if let Some(service) = services.get(&principal) {
            service.clone()
        } else {
            if services.len() >= 10000 {
                return (StatusCode::SERVICE_UNAVAILABLE, "MCP capacity reached").into_response();
            }
            let user = actor.user.clone();
            let store = s.store.clone();
            let url = s.security.base_url().to_string();
            let service = StreamableHttpService::new(
                move || {
                    Ok(CanvasMcp::new_user(
                        store.clone(),
                        url.clone(),
                        user.clone(),
                    ))
                },
                Arc::new(LocalSessionManager::default()),
                Default::default(),
            );
            services.insert(principal, service.clone());
            service
        }
    };
    service.handle(req).await.into_response()
}
async fn list(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
) -> Result<Json<Vec<pinboard_core::CanvasSummary>>, StoreError> {
    Ok(Json(workspace(&s, actor).list().await?))
}
async fn create(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Json(p): Json<Create>,
) -> Result<(StatusCode, Json<pinboard_core::Canvas>), StoreError> {
    Ok((
        StatusCode::CREATED,
        Json(
            workspace(&s, actor)
                .create(&p.title, p.nodes, p.edges)
                .await?,
        ),
    ))
}
async fn read(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
) -> Result<Json<pinboard_core::Canvas>, StoreError> {
    Ok(Json(workspace(&s, actor.clone()).read(&id).await?))
}
async fn edit(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    Json(batch): Json<Mutation>,
) -> Result<Json<pinboard_core::Canvas>, StoreError> {
    Ok(Json(workspace(&s, actor).edit(&id, &batch).await?))
}
async fn download(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path((id, format)): Path<(String, String)>,
) -> Result<Response, StoreError> {
    let c = workspace(&s, actor.clone()).read(&id).await?;
    let (mime, body) = export::render(&c, &format)
        .ok_or_else(|| CanvasError::Invalid("Unknown export format".into()))?;
    let extension = match format.as_str() {
        "markdown" => "md",
        "slides" => "html",
        "mermaid" => "mmd",
        _ => "json",
    };
    Ok((
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"pinboard.{extension}\""),
            ),
        ],
        body,
    )
        .into_response())
}
async fn live(
    State(s): State<AppState>,
    Extension(actor): Extension<Actor>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, StoreError> {
    workspace(&s, actor.clone()).read(&id).await?;
    Ok(ws
        .max_message_size(8192)
        .on_upgrade(move |socket| session(s, id, socket, headers)))
}
#[derive(Deserialize)]
struct Camera {
    x: f64,
    y: f64,
    zoom: f64,
    width: f64,
    height: f64,
}
async fn session(s: AppState, id: String, socket: WebSocket, headers: axum::http::HeaderMap) {
    let Ok(principal) = s.security.authenticate(&headers).await else {
        return;
    };
    let mut display_name = principal.user.name;
    let peer_id = new_id();
    let (mut tx, mut rx) = socket.split();
    let mut changes = s.store.changes.subscribe();
    let mut presence = s.presence.subscribe();
    let initial = s.peers.read().await.get(&id).cloned().unwrap_or_default();
    let _=tx.send(Message::Text(serde_json::json!({"type":"peers","peers":initial.values().collect::<Vec<_>>(),"self_id":peer_id}).to_string().into())).await;
    let mut poll = tokio::time::interval(Duration::from_secs(2));
    let mut revision = s.store.get(&id).await.map(|c| c.revision).unwrap_or(0);
    let mut last_role: Option<AccessRole> = None;
    let mut last_camera = tokio::time::Instant::now() - Duration::from_secs(1);
    loop {
        tokio::select! {
            incoming=rx.next()=>match incoming {
                Some(Ok(Message::Text(text)))=>{
                    if last_camera.elapsed()<Duration::from_millis(40) {continue;}
                    last_camera=tokio::time::Instant::now();
                    if let Ok(p)=serde_json::from_str::<Camera>(&text) {
                        if ![p.x,p.y,p.zoom,p.width,p.height].iter().all(|v|v.is_finite()) || !(1. ..=20000.).contains(&p.width) || !(1. ..=20000.).contains(&p.height) || p.x.abs()>1_000_000. || p.y.abs()>1_000_000. || !(0.1..=3.).contains(&p.zoom) {continue;}
                        let peer=Presence{id:peer_id.clone(),name:display_name.clone(),x:p.x,y:p.y,zoom:p.zoom,width:p.width,height:p.height};
                        let mut peers=s.peers.write().await; let room=peers.entry(id.clone()).or_default(); room.insert(peer_id.clone(),peer);
                        let _=s.presence.send(serde_json::json!({"type":"peers","canvas_id":id,"peers":room.values().collect::<Vec<_>>()} ).to_string());
                    }
                }
                Some(Ok(Message::Ping(bytes)))=>{if tx.send(Message::Pong(bytes)).await.is_err(){break;}},
                Some(Ok(Message::Close(_)))|Some(Err(_))|None=>break,
                _=>{}
            },
            change=changes.recv()=>{if let Ok(change)=change && change.canvas_id==id {revision=change.revision;if tx.send(Message::Text(serde_json::json!({"type":"canvas","revision":revision}).to_string().into())).await.is_err(){break;}}},
            message=presence.recv()=>{if let Ok(message)=message && serde_json::from_str::<serde_json::Value>(&message).ok().is_some_and(|m|m["canvas_id"]==id) && tx.send(Message::Text(message.into())).await.is_err(){break;}},
            _=poll.tick()=>{
                let Ok(actor)=s.security.authenticate(&headers).await else{break;};
                display_name=actor.user.name.clone();let workspace=Workspace::new(s.store.clone(),actor.user);let Ok(role)=workspace.role(&id).await else{break;};if last_role.as_ref()!=Some(&role){if tx.send(Message::Text(serde_json::json!({"type":"access","role":role}).to_string().into())).await.is_err(){break;}last_role=Some(role);}let Ok(current)=s.store.get(&id).await else{break;};
                // Cross-process writes and dropped broadcasts are recovered from durable state.
                if current.revision!=revision {revision=current.revision;if tx.send(Message::Text(serde_json::json!({"type":"canvas","revision":revision}).to_string().into())).await.is_err(){break;}}
                if tx.send(Message::Ping(vec![].into())).await.is_err(){break;}
            }
        }
    }
    let mut peers = s.peers.write().await;
    if let Some(room) = peers.get_mut(&id) {
        room.remove(&peer_id);
        let _=s.presence.send(serde_json::json!({"type":"peers","canvas_id":id,"peers":room.values().collect::<Vec<_>>()} ).to_string());
        if room.is_empty() {
            peers.remove(&id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use tower::ServiceExt;
    #[tokio::test]
    async fn routes_reject_hostile_origins_and_report_revision_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("http.db"), None, false)
            .await
            .unwrap();
        let c = store.create("API canvas", vec![], vec![]).await.unwrap();
        let app = router(
            store,
            dir.path().into(),
            "http://127.0.0.1:4127".into(),
            4127,
        );
        for (host, origin) in [
            ("attacker.example:4127", None),
            ("127.0.0.1:4127", Some("https://evil.example")),
        ] {
            let mut request = axum::http::Request::builder()
                .uri("/api/canvases")
                .header("host", host);
            if let Some(origin) = origin {
                request = request.header("origin", origin);
            }
            let r = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(r.status(), StatusCode::FORBIDDEN);
        }
        let body =
            serde_json::json!({"base_revision":0,"operations":[{"op":"rename","title":"Edited"}]})
                .to_string();
        for expected in [StatusCode::OK, StatusCode::CONFLICT] {
            let r = app
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri(format!("/api/canvases/{}/mutations", c.id))
                        .header("host", "127.0.0.1:4127")
                        .header("content-type", "application/json")
                        .body(Body::from(body.clone()))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(r.status(), expected);
            let bytes = to_bytes(r.into_body(), 10000).await.unwrap();
            if expected == StatusCode::CONFLICT {
                assert!(
                    String::from_utf8(bytes.to_vec())
                        .unwrap()
                        .contains("Revision conflict")
                );
            }
        }
        let missing = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/canvases/not-found")
                    .header("host", "localhost:4127")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }
}
