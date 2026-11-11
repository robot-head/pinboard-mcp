//! Configurable OIDC authorization-code + PKCE sign-in and revocable server sessions.
use crate::{
    collab::{local_user, random_token},
    models::{IdentityRecord, SessionRecord, UserRecord},
    store::{Result, Store, StoreError},
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, jwk::JwkSet};
use pinboard_core::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;

pub fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn unix() -> i64 {
    chrono::Utc::now().timestamp()
}
#[derive(Clone)]
pub struct Actor {
    pub user: UserInfo,
    pub expires_at: i64,
}
#[derive(Clone, Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    userinfo_endpoint: Option<String>,
}
#[derive(Clone)]
struct Oidc {
    client_id: String,
    client_secret: Option<String>,
    discovery: Discovery,
}
struct LoginAttempt {
    nonce: String,
    verifier: String,
    expires_at: i64,
    return_to: String,
}
#[derive(Clone)]
pub struct Security {
    store: Store,
    oidc: Option<Arc<Oidc>>,
    pub(crate) saml: Option<Arc<crate::saml::Saml>>,
    admin_emails: Vec<String>,
    internal_domains: Vec<String>,
    client: reqwest::Client,
    base_url: String,
    pending: Arc<Mutex<HashMap<String, LoginAttempt>>>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub id: String,
    pub expires_at: i64,
    pub kind: String,
}
impl Security {
    pub async fn from_env(store: Store, base_url: String) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let oidc = if let Ok(issuer) = std::env::var("PINBOARD_OIDC_ISSUER") {
            validate_endpoint(&issuer)?;
            let client_id = std::env::var("PINBOARD_OIDC_CLIENT_ID")
                .map_err(|_| anyhow::anyhow!("PINBOARD_OIDC_CLIENT_ID is required"))?;
            let discovery: Discovery = client
                .get(format!(
                    "{}/.well-known/openid-configuration",
                    issuer.trim_end_matches('/')
                ))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            anyhow::ensure!(
                discovery.issuer.trim_end_matches('/') == issuer.trim_end_matches('/'),
                "OIDC discovery issuer does not match configuration"
            );
            for endpoint in [
                &discovery.authorization_endpoint,
                &discovery.token_endpoint,
                &discovery.jwks_uri,
            ] {
                validate_endpoint(endpoint)?;
            }
            if let Some(endpoint) = &discovery.userinfo_endpoint {
                validate_endpoint(endpoint)?;
            }
            let admin_emails = list_env("PINBOARD_ADMIN_EMAILS");
            anyhow::ensure!(
                !admin_emails.is_empty(),
                "Configure PINBOARD_ADMIN_EMAILS for organization administration"
            );
            Some(Arc::new(Oidc {
                client_id,
                client_secret: std::env::var("PINBOARD_OIDC_CLIENT_SECRET").ok(),
                discovery,
            }))
        } else {
            None
        };
        let saml = crate::saml::Saml::from_env(&base_url, &client)
            .await?
            .map(Arc::new);
        let admin_emails = list_env("PINBOARD_ADMIN_EMAILS");
        let internal_domains = list_env("PINBOARD_INTERNAL_DOMAINS");
        anyhow::ensure!(
            (oidc.is_none() && saml.is_none()) || !admin_emails.is_empty(),
            "Configure PINBOARD_ADMIN_EMAILS for organization administration"
        );
        Ok(Self {
            store,
            oidc,
            saml,
            admin_emails,
            internal_domains,
            client,
            base_url,
            pending: Default::default(),
        })
    }
    #[cfg(test)]
    pub fn local(store: Store, base_url: String) -> Self {
        Self {
            store,
            oidc: None,
            saml: None,
            admin_emails: vec![],
            internal_domains: vec!["localhost".into()],
            client: reqwest::Client::new(),
            base_url,
            pending: Default::default(),
        }
    }
    pub fn configured(&self) -> bool {
        self.oidc.is_some() || self.saml.is_some()
    }
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
    pub fn internal_domains(&self) -> Vec<String> {
        if self.configured() {
            self.internal_domains.clone()
        } else {
            vec!["localhost".into()]
        }
    }
    pub fn providers(&self) -> Vec<LoginProvider> {
        let mut providers = vec![];
        if self.oidc.is_some() {
            providers.push(LoginProvider {
                label: std::env::var("PINBOARD_OIDC_LABEL").unwrap_or_else(|_| "OIDC".into()),
                login_url: "/auth/login".into(),
            });
        }
        if self.saml.is_some() {
            providers.push(LoginProvider {
                label: std::env::var("PINBOARD_SAML_LABEL").unwrap_or_else(|_| "SAML".into()),
                login_url: "/auth/saml/login".into(),
            });
        }
        providers
    }
    pub async fn authenticate(&self, headers: &HeaderMap) -> Result<Actor> {
        if !self.configured() {
            return Ok(Actor {
                user: local_user(),
                expires_at: i64::MAX,
            });
        }
        let token = bearer_or_cookie(headers).ok_or(StoreError::Unauthorized)?;
        let mut db = self.store.db.clone();
        let session = SessionRecord::filter_by_id(digest(&token))
            .first()
            .exec(&mut db)
            .await?
            .filter(|s| s.expires_at > unix())
            .ok_or(StoreError::Unauthorized)?;
        let mut user: UserInfo = serde_json::from_str(&session.document)?;
        self.apply_roles(&mut user);
        Ok(Actor {
            user,
            expires_at: session.expires_at,
        })
    }
    fn apply_roles(&self, user: &mut UserInfo) {
        if self.configured() {
            user.admin = self
                .admin_emails
                .iter()
                .any(|e| e.eq_ignore_ascii_case(&user.email));
            let domain = user.email.rsplit_once('@').map(|(_, d)| d).unwrap_or("");
            user.internal = user.admin
                || self
                    .internal_domains
                    .iter()
                    .any(|d| d.eq_ignore_ascii_case(domain));
        }
    }
    async fn issue(
        &self,
        user: &UserInfo,
        kind: &str,
        lifetime: i64,
    ) -> Result<(String, TokenInfo)> {
        let _writer = self.store.write_lock.lock().await;
        let token = random_token();
        let id = digest(&token);
        let expires_at = unix() + lifetime;
        toasty::create!(SessionRecord {
            id: id.clone(),
            user_id: user.id.clone(),
            document: serde_json::to_string(user)?,
            expires_at,
            kind
        })
        .exec(&mut self.store.db.clone())
        .await?;
        Ok((
            token,
            TokenInfo {
                id,
                expires_at,
                kind: kind.into(),
            },
        ))
    }
    pub async fn api_token(&self, user: &UserInfo) -> Result<(String, TokenInfo)> {
        self.issue(user, "api", 30 * 86400).await
    }
    pub async fn tokens(&self, user: &UserInfo) -> Result<Vec<TokenInfo>> {
        Ok(SessionRecord::filter_by_user_id(&user.id)
            .exec(&mut self.store.db.clone())
            .await?
            .into_iter()
            .filter(|s| s.expires_at > unix() && s.kind == "api")
            .map(|s| TokenInfo {
                id: s.id,
                expires_at: s.expires_at,
                kind: s.kind,
            })
            .collect())
    }
    pub async fn revoke(&self, user: &UserInfo, id: &str) -> Result<()> {
        let _writer = self.store.write_lock.lock().await;
        let mut db = self.store.db.clone();
        if let Some(session) = SessionRecord::filter_by_id(id)
            .first()
            .exec(&mut db)
            .await?
        {
            if session.user_id != user.id {
                return Err(StoreError::Forbidden(
                    "Cannot revoke another user's session".into(),
                ));
            }
            SessionRecord::filter_by_id(id)
                .delete()
                .exec(&mut db)
                .await?;
        }
        Ok(())
    }
    async fn persist_identity(&self, identity: ExternalIdentity) -> Result<UserInfo> {
        let _writer = self.store.write_lock.lock().await;
        let mut db = self.store.db.clone();
        let mut tx = db
            .transaction_builder()
            .mode(toasty_core::driver::operation::TransactionMode::Immediate)
            .begin()
            .await?;
        let binding = IdentityRecord::filter_by_id(&identity.identity_id)
            .first()
            .exec(&mut tx)
            .await?;
        let existing = if let Some(binding) = &binding {
            UserRecord::filter_by_id(&binding.user_id)
                .first()
                .exec(&mut tx)
                .await?
        } else {
            UserRecord::filter_by_email(&identity.email)
                .first()
                .exec(&mut tx)
                .await?
        };
        let mut user = if let Some(record) = &existing {
            serde_json::from_str::<UserInfo>(&record.document)?
        } else {
            UserInfo {
                id: new_id(),
                email: identity.email.clone(),
                name: identity.name.clone(),
                admin: false,
                internal: false,
            }
        };
        if let Some(other) = UserRecord::filter_by_email(&identity.email)
            .first()
            .exec(&mut tx)
            .await?
            && other.id != user.id
        {
            return Err(StoreError::Forbidden(
                "This verified email is already linked to another account".into(),
            ));
        }
        user.email = identity.email;
        user.name = identity.name.chars().take(80).collect();
        self.apply_roles(&mut user);
        if let Some(mut record) = existing {
            toasty::update!(record {
                email: user.email.clone(),
                document: serde_json::to_string(&user)?
            })
            .exec(&mut tx)
            .await?;
        } else {
            toasty::create!(UserRecord {
                id: user.id.clone(),
                email: user.email.clone(),
                document: serde_json::to_string(&user)?
            })
            .exec(&mut tx)
            .await?;
        }
        if binding.is_none() {
            toasty::create!(IdentityRecord {
                id: identity.identity_id,
                user_id: user.id.clone()
            })
            .exec(&mut tx)
            .await?;
        }
        tx.commit().await?;
        Ok(user)
    }
    pub(crate) async fn finish_external(&self, identity: ExternalIdentity) -> Result<String> {
        let user = self.persist_identity(identity).await?;
        let (token, _) = self.issue(&user, "browser", 12 * 3600).await?;
        Ok(self.cookie(&token, 12 * 3600))
    }
    fn cookie(&self, value: &str, max_age: i64) -> String {
        format!(
            "pinboard_session={value}; HttpOnly; Path=/; SameSite=Lax; Max-Age={max_age}{}",
            if self.base_url.starts_with("https:") {
                "; Secure"
            } else {
                ""
            }
        )
    }
    async fn begin(&self, return_to: String) -> Result<(String, String)> {
        let Some(o) = &self.oidc else {
            return Ok((self.base_url.clone(), String::new()));
        };
        let state = random_token();
        let nonce = random_token();
        let verifier = random_token();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let mut url = url::Url::parse(&o.discovery.authorization_endpoint)
            .map_err(|e| StoreError::Internal(e.into()))?;
        url.query_pairs_mut().extend_pairs([
            ("response_type", "code"),
            ("client_id", &o.client_id),
            ("redirect_uri", &format!("{}/auth/callback", self.base_url)),
            ("scope", "openid profile email"),
            ("state", &state),
            ("nonce", &nonce),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ]);
        let mut pending = self.pending.lock().await;
        pending.retain(|_, a| a.expires_at > unix());
        if pending.len() >= 1000 {
            return Err(StoreError::Unavailable(
                "Too many pending sign-in attempts".into(),
            ));
        }
        let return_to = if return_to.starts_with('/')
            && !return_to.starts_with("//")
            && !return_to.contains('\\')
            && !return_to.chars().any(char::is_control)
        {
            return_to
        } else {
            "/".into()
        };
        pending.insert(
            state.clone(),
            LoginAttempt {
                nonce,
                verifier,
                expires_at: unix() + 300,
                return_to,
            },
        );
        Ok((url.into(), state))
    }
    async fn finish(&self, p: Callback) -> Result<(String, String)> {
        let o = self.oidc.as_ref().ok_or(StoreError::Unauthorized)?;
        let attempt = self
            .pending
            .lock()
            .await
            .remove(&p.state)
            .filter(|a| a.expires_at > unix())
            .ok_or(StoreError::Unauthorized)?;
        let mut form = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", p.code),
            ("client_id", o.client_id.clone()),
            ("redirect_uri", format!("{}/auth/callback", self.base_url)),
            ("code_verifier", attempt.verifier),
        ];
        if let Some(secret) = &o.client_secret {
            form.push(("client_secret", secret.clone()));
        }
        let tokens: TokenResponse = self
            .client
            .post(&o.discovery.token_endpoint)
            .form(&form)
            .send()
            .await
            .map_err(internal)?
            .error_for_status()
            .map_err(internal)?
            .json()
            .await
            .map_err(internal)?;
        let header =
            jsonwebtoken::decode_header(&tokens.id_token).map_err(|_| StoreError::Unauthorized)?;
        if !matches!(
            header.alg,
            Algorithm::RS256
                | Algorithm::RS384
                | Algorithm::RS512
                | Algorithm::ES256
                | Algorithm::ES384
                | Algorithm::EdDSA
        ) {
            return Err(StoreError::Unauthorized);
        }
        let keys: JwkSet = self
            .client
            .get(&o.discovery.jwks_uri)
            .send()
            .await
            .map_err(internal)?
            .error_for_status()
            .map_err(internal)?
            .json()
            .await
            .map_err(internal)?;
        let key = keys
            .keys
            .iter()
            .find(|k| k.common.key_id == header.kid)
            .or_else(|| (header.kid.is_none() && keys.keys.len() == 1).then(|| &keys.keys[0]))
            .ok_or(StoreError::Unauthorized)?;
        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[&o.discovery.issuer]);
        validation.set_audience(&[&o.client_id]);
        validation.leeway = 30;
        let claims = jsonwebtoken::decode::<Claims>(
            &tokens.id_token,
            &DecodingKey::from_jwk(key).map_err(|_| StoreError::Unauthorized)?,
            &validation,
        )
        .map_err(|_| StoreError::Unauthorized)?
        .claims;
        let authorized_party = match &claims.aud {
            Audience::Single(audience) => audience == &o.client_id,
            Audience::Multiple(audiences) => {
                audiences.contains(&o.client_id)
                    && (audiences.len() == 1 || claims.azp.as_deref() == Some(&o.client_id))
            }
        } && claims
            .azp
            .as_deref()
            .is_none_or(|party| party == o.client_id);
        if !authorized_party
            || claims.nonce.as_deref() != Some(&attempt.nonce)
            || claims.sub.is_empty()
        {
            return Err(StoreError::Unauthorized);
        }
        let (email, verified, name) = if claims.email.is_some() {
            (claims.email, claims.email_verified, claims.name)
        } else if let (Some(endpoint), Some(access_token)) =
            (&o.discovery.userinfo_endpoint, tokens.access_token)
        {
            let info: UserClaims = self
                .client
                .get(endpoint)
                .bearer_auth(access_token)
                .send()
                .await
                .map_err(internal)?
                .error_for_status()
                .map_err(internal)?
                .json()
                .await
                .map_err(internal)?;
            if info.sub != claims.sub {
                return Err(StoreError::Unauthorized);
            }
            (info.email, info.email_verified, info.name)
        } else {
            return Err(StoreError::Unauthorized);
        };
        let email = email
            .filter(|e| e.len() <= 254 && e.parse::<lettre::Address>().is_ok())
            .ok_or(StoreError::Unauthorized)?
            .to_lowercase();
        if verified != Some(true) {
            return Err(StoreError::Forbidden(
                "Your identity provider must supply a verified email address".into(),
            ));
        }
        let session = self
            .finish_external(ExternalIdentity {
                identity_id: digest(&format!("oidc:{}:{}", o.discovery.issuer, claims.sub)),
                email,
                name: name.unwrap_or_else(|| "Workspace user".into()),
            })
            .await?;
        Ok((session, attempt.return_to))
    }
}
fn internal(error: impl std::error::Error + Send + Sync + 'static) -> StoreError {
    StoreError::Internal(anyhow::Error::new(error))
}
fn list_env(name: &str) -> Vec<String> {
    std::env::var(name)
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}
pub(crate) fn validate_endpoint(endpoint: &str) -> anyhow::Result<()> {
    let url = url::Url::parse(endpoint)?;
    anyhow::ensure!(
        url.username().is_empty() && url.password().is_none() && url.fragment().is_none(),
        "Identity-provider endpoints must not contain credentials or fragments"
    );
    let local = matches!(
        url.host_str(),
        Some("127.0.0.1" | "localhost" | "[::1]" | "::1")
    );
    anyhow::ensure!(
        url.scheme() == "https" || (url.scheme() == "http" && local),
        "OIDC endpoints require HTTPS except on loopback"
    );
    Ok(())
}
fn bearer_or_cookie(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
    {
        return Some(value.into());
    }
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|s| {
            s.trim()
                .strip_prefix("pinboard_session=")
                .map(str::to_string)
        })
}
#[derive(Deserialize)]
struct Claims {
    sub: String,
    aud: Audience,
    azp: Option<String>,
    nonce: Option<String>,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    Single(String),
    Multiple(Vec<String>),
}
#[derive(Deserialize)]
struct UserClaims {
    sub: String,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
}
#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
    access_token: Option<String>,
}
#[derive(Deserialize)]
pub struct LoginQuery {
    #[serde(default)]
    return_to: String,
}
#[derive(Deserialize)]
pub struct Callback {
    code: String,
    state: String,
}
pub async fn login(
    State(s): State<crate::http::AppState>,
    Query(p): Query<LoginQuery>,
) -> Result<Response> {
    let (url, state) = s.security.begin(p.return_to).await?;
    let cookie = format!(
        "pinboard_oidc_state={state}; HttpOnly; Path=/auth; SameSite=Lax; Max-Age=300{}",
        if s.security.base_url.starts_with("https:") {
            "; Secure"
        } else {
            ""
        }
    );
    Ok(([(header::SET_COOKIE, cookie)], Redirect::to(&url)).into_response())
}
pub async fn callback(
    State(s): State<crate::http::AppState>,
    headers: HeaderMap,
    Query(p): Query<Callback>,
) -> Result<Response> {
    let bound = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.split(';')
                .find_map(|s| s.trim().strip_prefix("pinboard_oidc_state="))
        });
    if bound != Some(p.state.as_str()) {
        return Err(StoreError::Unauthorized);
    }
    let (cookie, path) = s.security.finish(p).await?;
    Ok((
        axum::response::AppendHeaders([
            (header::SET_COOKIE, cookie),
            (
                header::SET_COOKIE,
                "pinboard_oidc_state=; HttpOnly; Path=/auth; SameSite=Lax; Max-Age=0".into(),
            ),
        ]),
        Redirect::to(&path),
    )
        .into_response())
}
pub async fn logout(
    State(s): State<crate::http::AppState>,
    headers: HeaderMap,
) -> Result<Response> {
    if let Ok(actor) = s.security.authenticate(&headers).await
        && let Some(token) = bearer_or_cookie(&headers)
    {
        s.security.revoke(&actor.user, &digest(&token)).await?;
    }
    Ok((
        [(header::SET_COOKIE, s.security.cookie("", 0))],
        Redirect::to("/"),
    )
        .into_response())
}

pub(crate) struct ExternalIdentity {
    pub identity_id: String,
    pub email: String,
    pub name: String,
}
pub(crate) fn safe_return_path(path: String) -> String {
    if path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
    {
        path
    } else {
        "/".into()
    }
}

#[cfg(test)]
#[path = "security_tests.rs"]
mod tests;
