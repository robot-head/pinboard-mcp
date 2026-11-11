use super::*;
fn configured(store: Store) -> Security {
    let mut security = Security::local(store, "http://127.0.0.1:4127".into());
    security.oidc = Some(Arc::new(Oidc {
        client_id: "fixture".into(),
        client_secret: None,
        discovery: Discovery {
            issuer: "https://idp.example".into(),
            authorization_endpoint: "https://idp.example/auth".into(),
            token_endpoint: "https://idp.example/token".into(),
            jwks_uri: "https://idp.example/keys".into(),
            userinfo_endpoint: None,
        },
    }));
    security.admin_emails = vec!["admin@example.com".into()];
    security.internal_domains = vec!["example.com".into()];
    security
}
#[tokio::test]
async fn verified_identities_link_to_one_account_and_tokens_are_revocable() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("identity.db"), None, false)
        .await
        .unwrap();
    let security = configured(store.clone());
    let first = security
        .persist_identity(ExternalIdentity {
            identity_id: "oidc-subject".into(),
            email: "admin@example.com".into(),
            name: "Admin".into(),
        })
        .await
        .unwrap();
    let linked = security
        .persist_identity(ExternalIdentity {
            identity_id: "saml-subject".into(),
            email: "admin@example.com".into(),
            name: "Admin via SAML".into(),
        })
        .await
        .unwrap();
    assert_eq!(first.id, linked.id);
    assert!(linked.admin && linked.internal);
    let (token, info) = security.api_token(&linked).await.unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    assert_eq!(
        security.authenticate(&headers).await.unwrap().user.id,
        first.id
    );
    security.revoke(&linked, &info.id).await.unwrap();
    assert!(matches!(
        security.authenticate(&headers).await,
        Err(StoreError::Unauthorized)
    ));
    let (token, info) = security.api_token(&linked).await.unwrap();
    let mut record = SessionRecord::filter_by_id(info.id)
        .get(&mut store.db.clone())
        .await
        .unwrap();
    toasty::update!(record {
        expires_at: unix() - 1
    })
    .exec(&mut store.db.clone())
    .await
    .unwrap();
    headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {token}").parse().unwrap(),
    );
    assert!(security.authenticate(&headers).await.is_err());
}
#[test]
fn return_paths_and_provider_endpoints_are_constrained() {
    for path in [
        "//evil.example",
        "/\\evil.example",
        "/\nlocation:evil",
        "https://evil.example",
    ] {
        assert_eq!(safe_return_path(path.into()), "/");
    }
    assert_eq!(safe_return_path("/?canvas=abc".into()), "/?canvas=abc");
    assert!(validate_endpoint("http://idp.example").is_err());
    assert!(validate_endpoint("https://idp.example").is_ok());
    assert!(validate_endpoint("http://127.0.0.1:8000").is_ok());
}
