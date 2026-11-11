//! SAML SP integration. Signature reduction and SAML validation are delegated to
//! Samael/xmlsec; only authenticated, signed assertion attributes reach accounts.
use crate::{
    collab::random_token,
    security::{ExternalIdentity, digest},
    store::{Result, StoreError},
};
use axum::{
    extract::{Form, Query, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Redirect, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use samael::{
    crypto::{AllowedSignatureAlgorithm, CertificateDer},
    key_info::{KeyInfo, X509Data},
    metadata::{EntityDescriptor, IndexedEndpoint, KeyDescriptor, SpSsoDescriptor},
    service_provider::ServiceProvider,
    traits::ToXml,
};
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

struct Pending {
    request_id: String,
    expires_at: i64,
    return_to: String,
}
#[derive(Clone)]
pub struct Saml {
    sp: ServiceProvider,
    sso_url: String,
    email_attribute: String,
    name_attribute: String,
    pending: Arc<Mutex<HashMap<String, Pending>>>,
    base_url: String,
}
impl Saml {
    pub async fn from_env(
        base_url: &str,
        client: &reqwest::Client,
    ) -> anyhow::Result<Option<Self>> {
        let metadata = if let Ok(path) = std::env::var("PINBOARD_SAML_IDP_METADATA_FILE") {
            std::fs::read_to_string(path)?
        } else if let Ok(endpoint) = std::env::var("PINBOARD_SAML_IDP_METADATA_URL") {
            crate::security::validate_endpoint(&endpoint)?;
            client
                .get(endpoint)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?
        } else {
            return Ok(None);
        };
        safe_xml(&metadata)
            .map_err(|_| anyhow::anyhow!("SAML metadata contains unsupported XML"))?;
        let mut idp: EntityDescriptor = metadata
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid SAML IdP metadata: {e}"))?;
        anyhow::ensure!(
            idp.entity_id.as_ref().is_some_and(|s| !s.is_empty()),
            "SAML IdP metadata requires an entityID"
        );
        anyhow::ensure!(
            idp.valid_until.is_none_or(|time| time > chrono::Utc::now()),
            "SAML IdP metadata has expired"
        );
        // SAML metadata without `use` declares a key valid for both purposes.
        for descriptor in idp.idp_sso_descriptors.iter_mut().flatten() {
            for key in &mut descriptor.key_descriptors {
                if key.key_use.is_none() {
                    key.key_use = Some("signing".into());
                }
            }
        }
        let key_path = std::env::var("PINBOARD_SAML_SP_KEY_FILE").map_err(|_| {
            anyhow::anyhow!("PINBOARD_SAML_SP_KEY_FILE is required for signed SAML requests")
        })?;
        let certificate_path = std::env::var("PINBOARD_SAML_SP_CERT_FILE")
            .map_err(|_| anyhow::anyhow!("PINBOARD_SAML_SP_CERT_FILE is required"))?;
        let key = openssl::pkey::PKey::private_key_from_pem(&std::fs::read(key_path)?)?;
        let cert = openssl::x509::X509::from_pem(&std::fs::read(certificate_path)?)?;
        let public_key = cert.public_key()?;
        anyhow::ensure!(
            key.public_eq(public_key.as_ref()),
            "SAML SP certificate does not match its signing key"
        );
        anyhow::ensure!(
            key.id() == openssl::pkey::Id::RSA && key.bits() >= 2048,
            "Use an RSA SP signing key of at least 2048 bits"
        );
        let sp = ServiceProvider {
            entity_id: Some(
                std::env::var("PINBOARD_SAML_ENTITY_ID")
                    .unwrap_or_else(|_| format!("{base_url}/auth/saml/metadata")),
            ),
            key: Some(key),
            certificate: Some(CertificateDer::from(cert.to_der()?)),
            metadata_url: Some(format!("{base_url}/auth/saml/metadata")),
            acs_url: Some(format!("{base_url}/auth/saml/acs")),
            idp_metadata: idp,
            slo_url: None,
            authn_name_id_format: Some(
                std::env::var("PINBOARD_SAML_NAME_ID_FORMAT").unwrap_or_else(|_| {
                    "urn:oasis:names:tc:SAML:2.0:nameid-format:persistent".into()
                }),
            ),
            allow_idp_initiated: false,
            max_issue_delay: chrono::Duration::minutes(5),
            max_clock_skew: chrono::Duration::seconds(30),
            allowed_signature_algorithms: Some(vec![
                AllowedSignatureAlgorithm::RsaSha256,
                AllowedSignatureAlgorithm::RsaSha384,
                AllowedSignatureAlgorithm::RsaSha512,
                AllowedSignatureAlgorithm::EcdsaSha256,
                AllowedSignatureAlgorithm::EcdsaSha384,
                AllowedSignatureAlgorithm::EcdsaSha512,
            ]),
            ..Default::default()
        };
        anyhow::ensure!(
            sp.idp_signing_certs()
                .map_err(|e| anyhow::anyhow!(e.to_string()))?
                .is_some_and(|c| !c.is_empty()),
            "SAML metadata must include trusted IdP signing certificates"
        );
        let sso_url = sp
            .sso_binding_location("urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect")
            .ok_or_else(|| {
                anyhow::anyhow!("SAML IdP must support HTTP-Redirect AuthnRequest binding")
            })?;
        crate::security::validate_endpoint(&sso_url)?;
        Ok(Some(Self {
            sp,
            sso_url,
            email_attribute: std::env::var("PINBOARD_SAML_EMAIL_ATTRIBUTE")
                .unwrap_or_else(|_| "email".into()),
            name_attribute: std::env::var("PINBOARD_SAML_NAME_ATTRIBUTE")
                .unwrap_or_else(|_| "displayName".into()),
            pending: Default::default(),
            base_url: base_url.into(),
        }))
    }
    async fn begin(&self, return_to: String) -> Result<(String, String)> {
        let state = random_token();
        let mut request = self
            .sp
            .make_authentication_request(&self.sso_url)
            .map_err(|_| {
                StoreError::Unavailable("Could not create SAML authentication request".into())
            })?;
        request.id = format!("_{}", digest(&random_token()));
        let url = request
            .signed_redirect(&state, self.sp.key.as_ref().unwrap())
            .map_err(|_| {
                StoreError::Unavailable("Could not sign SAML authentication request".into())
            })?
            .ok_or_else(|| StoreError::Unavailable("SAML SSO endpoint is missing".into()))?;
        let mut pending = self.pending.lock().await;
        pending.retain(|_, p| p.expires_at > chrono::Utc::now().timestamp());
        if pending.len() >= 1000 {
            return Err(StoreError::Unavailable(
                "Too many pending sign-in attempts".into(),
            ));
        }
        pending.insert(
            state.clone(),
            Pending {
                request_id: request.id,
                expires_at: chrono::Utc::now().timestamp() + 300,
                return_to: crate::security::safe_return_path(return_to),
            },
        );
        Ok((url.into(), state))
    }
    async fn finish(&self, state: String, response: String) -> Result<(ExternalIdentity, String)> {
        let pending = self
            .pending
            .lock()
            .await
            .remove(&state)
            .filter(|p| p.expires_at > chrono::Utc::now().timestamp())
            .ok_or(StoreError::Unauthorized)?;
        if response.len() > 3_000_000 {
            return Err(StoreError::Unauthorized);
        }
        let xml = STANDARD
            .decode(response)
            .map_err(|_| StoreError::Unauthorized)?;
        let xml = String::from_utf8(xml).map_err(|_| StoreError::Unauthorized)?;
        safe_xml(&xml)?;
        let sp = self.sp.clone();
        let email_attribute = self.email_attribute.clone();
        let name_attribute = self.name_attribute.clone();
        let request_id = pending.request_id;
        let identity = tokio::task::spawn_blocking(move || {
            let assertion = sp
                .parse_xml_response(&xml, Some(&[request_id.as_str()]))
                .map_err(|_| StoreError::Unauthorized)?;
            if assertion.issue_instant > chrono::Utc::now() + sp.max_clock_skew {
                return Err(StoreError::Unauthorized);
            }
            let conditions = assertion
                .conditions
                .as_ref()
                .ok_or(StoreError::Unauthorized)?;
            let entity = sp.entity_id.as_deref().ok_or(StoreError::Unauthorized)?;
            if conditions.not_on_or_after.is_none()
                || !conditions
                    .audience_restrictions
                    .as_ref()
                    .is_some_and(|all| {
                        !all.is_empty()
                            && all.iter().all(|r| r.audience.iter().any(|a| a == entity))
                    })
            {
                return Err(StoreError::Unauthorized);
            }
            let subject = assertion.subject.as_ref().ok_or(StoreError::Unauthorized)?;
            let name_id = subject
                .name_id
                .as_ref()
                .filter(|n| !n.value.is_empty())
                .ok_or(StoreError::Unauthorized)?;
            if !subject.subject_confirmations.as_ref().is_some_and(|all| {
                all.iter().any(|s| {
                    s.subject_confirmation_data.as_ref().is_some_and(|d| {
                        d.not_on_or_after.is_some()
                            && d.in_response_to.as_deref() == Some(request_id.as_str())
                            && d.recipient.as_deref() == sp.acs_url.as_deref()
                    })
                })
            }) {
                return Err(StoreError::Unauthorized);
            }
            let attribute = |name: &str| {
                assertion
                    .attribute_statements
                    .as_ref()
                    .into_iter()
                    .flatten()
                    .flat_map(|s| s.attributes.iter())
                    .filter(|a| {
                        a.name.as_deref() == Some(name) || a.friendly_name.as_deref() == Some(name)
                    })
                    .flat_map(|a| a.values.iter())
                    .filter_map(|v| v.value.clone())
                    .collect::<Vec<_>>()
            };
            let emails = attribute(&email_attribute);
            if emails.len() != 1 {
                return Err(StoreError::Unauthorized);
            }
            let email = emails[0].trim().to_lowercase();
            if email.len() > 254 || email.parse::<lettre::Address>().is_err() {
                return Err(StoreError::Unauthorized);
            }
            let name = attribute(&name_attribute)
                .first()
                .cloned()
                .unwrap_or_else(|| email.clone());
            let issuer = assertion.issuer.value.ok_or(StoreError::Unauthorized)?;
            Ok(ExternalIdentity {
                identity_id: digest(&format!("saml:{issuer}:{}", name_id.value)),
                email,
                name,
            })
        })
        .await
        .map_err(|e| StoreError::Internal(e.into()))??;
        Ok((identity, pending.return_to))
    }
    pub fn metadata(&self) -> Result<String> {
        // Samael's convenience builder requires SLO and declares unsigned
        // requests. Advertise only the bindings this service implements.
        EntityDescriptor {
            entity_id: self.sp.entity_id.clone(),
            sp_sso_descriptors: Some(vec![SpSsoDescriptor {
                protocol_support_enumeration: Some("urn:oasis:names:tc:SAML:2.0:protocol".into()),
                authn_requests_signed: Some(true),
                want_assertions_signed: Some(true),
                key_descriptors: Some(vec![KeyDescriptor {
                    key_use: Some("signing".into()),
                    key_info: KeyInfo {
                        id: None,
                        x509_data: Some(X509Data {
                            certificates: vec![
                                STANDARD.encode(self.sp.certificate.as_ref().unwrap().der_data()),
                            ],
                        }),
                    },
                    encryption_methods: None,
                }]),
                assertion_consumer_services: vec![IndexedEndpoint {
                    binding: "urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST".into(),
                    location: self.sp.acs_url.clone().unwrap(),
                    ..Default::default()
                }],
                ..Default::default()
            }]),
            ..Default::default()
        }
        .to_string()
        .map_err(|_| StoreError::Unavailable("Could not generate SAML SP metadata".into()))
    }
    fn cookie(&self, state: &str, max_age: i64) -> String {
        format!(
            "pinboard_saml_state={state}; HttpOnly; Path=/auth/saml; Max-Age={max_age}; {}",
            if self.base_url.starts_with("https:") {
                "SameSite=None; Secure"
            } else {
                "SameSite=Lax"
            }
        )
    }
}

/// Constrain XML to SAML's inline signature profile before xmlsec processes it.
fn safe_xml(xml: &str) -> Result<()> {
    if xml.len() > 2_000_000
        || xml.to_ascii_uppercase().contains("<!DOCTYPE")
        || xml.to_ascii_uppercase().contains("<!ENTITY")
    {
        return Err(StoreError::Unauthorized);
    }
    let mut reader = quick_xml::Reader::from_str(xml);
    loop {
        match reader.read_event().map_err(|_| StoreError::Unauthorized)? {
            quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e) => {
                let name = e.local_name();
                let name = name.as_ref();
                if matches!(name, b"RetrievalMethod" | b"KeyInfoReference") {
                    return Err(StoreError::Unauthorized);
                }
                for attribute in e.attributes() {
                    let a = attribute.map_err(|_| StoreError::Unauthorized)?;
                    if name == b"Reference"
                        && a.key.as_ref() == b"URI"
                        && !a.value.starts_with(b"#")
                    {
                        return Err(StoreError::Unauthorized);
                    }
                    if name == b"DigestMethod"
                        && a.key.as_ref() == b"Algorithm"
                        && ![
                            "http://www.w3.org/2001/04/xmlenc#sha256",
                            "http://www.w3.org/2001/04/xmldsig-more#sha384",
                            "http://www.w3.org/2001/04/xmlenc#sha512",
                        ]
                        .iter()
                        .any(|v| a.value.as_ref() == v.as_bytes())
                    {
                        return Err(StoreError::Unauthorized);
                    }
                    if matches!(name, b"Transform" | b"CanonicalizationMethod")
                        && a.key.as_ref() == b"Algorithm"
                    {
                        let allowed = [
                            "http://www.w3.org/2000/09/xmldsig#enveloped-signature",
                            "http://www.w3.org/2001/10/xml-exc-c14n#",
                            "http://www.w3.org/2001/10/xml-exc-c14n#WithComments",
                            "http://www.w3.org/TR/2001/REC-xml-c14n-20010315",
                            "http://www.w3.org/TR/2001/REC-xml-c14n-20010315#WithComments",
                            "http://www.w3.org/2006/12/xml-c14n11",
                            "http://www.w3.org/2006/12/xml-c14n11#WithComments",
                        ];
                        if !allowed.iter().any(|v| a.value.as_ref() == v.as_bytes()) {
                            return Err(StoreError::Unauthorized);
                        }
                    }
                }
            }
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}
#[derive(Deserialize)]
pub struct SamlLogin {
    #[serde(default)]
    return_to: String,
}
#[derive(Deserialize)]
pub struct SamlResponse {
    #[serde(rename = "SAMLResponse")]
    response: String,
    #[serde(rename = "RelayState")]
    relay_state: String,
}
pub async fn metadata(State(s): State<crate::http::AppState>) -> Result<Response> {
    let saml = s.security.saml.as_ref().ok_or(StoreError::Unauthorized)?;
    Ok((
        [(header::CONTENT_TYPE, "application/samlmetadata+xml")],
        saml.metadata()?,
    )
        .into_response())
}
pub async fn login(
    State(s): State<crate::http::AppState>,
    Query(p): Query<SamlLogin>,
) -> Result<Response> {
    let saml = s.security.saml.as_ref().ok_or(StoreError::Unauthorized)?;
    let (url, state) = saml.begin(p.return_to).await?;
    Ok((
        [(header::SET_COOKIE, saml.cookie(&state, 300))],
        Redirect::to(&url),
    )
        .into_response())
}
pub async fn acs(
    State(s): State<crate::http::AppState>,
    headers: HeaderMap,
    Form(p): Form<SamlResponse>,
) -> Result<Response> {
    let saml = s.security.saml.as_ref().ok_or(StoreError::Unauthorized)?;
    let cookie = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.split(';')
                .find_map(|s| s.trim().strip_prefix("pinboard_saml_state="))
        });
    if cookie != Some(p.relay_state.as_str()) {
        return Err(StoreError::Unauthorized);
    }
    let (identity, path) = saml.finish(p.relay_state, p.response).await?;
    let session = s.security.finish_external(identity).await?;
    Ok((
        axum::response::AppendHeaders([
            (header::SET_COOKIE, session),
            (header::SET_COOKIE, saml.cookie("", 0)),
        ]),
        Redirect::to(&path),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_dtd_remote_references_and_xslt() {
        for xml in [
            "<!DOCTYPE x><x/>",
            "<Reference URI='file:///etc/passwd'/>",
            "<Transform Algorithm='http://www.w3.org/TR/1999/REC-xslt-19991116'/>",
            "<RetrievalMethod URI='https://evil.test/key'/>",
            "<DigestMethod Algorithm='http://www.w3.org/2000/09/xmldsig#sha1'/>",
        ] {
            assert!(safe_xml(xml).is_err());
        }
        assert!(safe_xml("<Reference URI='#signed'/>").is_ok());
    }
}
