use super::*;
fn person(id: &str, email: &str) -> UserInfo {
    UserInfo {
        id: id.into(),
        email: email.into(),
        name: id.into(),
        admin: false,
        internal: true,
    }
}
async fn fixture() -> (tempfile::TempDir, Store, Workspace, Canvas) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("team.db"), None, false)
        .await
        .unwrap();
    let owner = Workspace::new(store.clone(), person("owner", "owner@example.com"));
    let canvas = owner
        .create("Shared", vec![Node::note("Review", 100., 100.)], vec![])
        .await
        .unwrap();
    (dir, store, owner, canvas)
}
async fn grant(store: &Store, canvas: &str, user: &str, role: AccessRole) {
    toasty::create!(GrantRecord {
        id: format!("{canvas}:{user}"),
        canvas_id: canvas,
        subject: user,
        role: role_string(&role)
    })
    .exec(&mut store.db.clone())
    .await
    .unwrap();
}
#[tokio::test]
async fn permissions_are_enforced_inside_the_write_transaction() {
    let (_dir, store, owner, c) = fixture().await;
    let editor = Workspace::new(store.clone(), person("editor", "editor@example.com"));
    assert!(editor.read(&c.id).await.is_err());
    grant(&store, &c.id, "editor", AccessRole::Editor).await;
    editor
        .edit(
            &c.id,
            &Mutation {
                base_revision: 0,
                scope: None,
                operations: vec![Operation::Rename {
                    title: "Allowed".into(),
                }],
            },
        )
        .await
        .unwrap();
    owner.revoke_member(&c.id, "editor").await.unwrap();
    let result = store
        .mutate_as(
            &editor.user,
            &c.id,
            &Mutation {
                base_revision: 1,
                scope: None,
                operations: vec![Operation::Rename {
                    title: "Forbidden".into(),
                }],
            },
        )
        .await;
    assert!(matches!(result, Err(StoreError::Forbidden(_))));
    assert_eq!(store.get(&c.id).await.unwrap().title, "Allowed");
}
#[tokio::test]
async fn invitations_bind_delivery_expiry_and_verified_recipient() {
    let (_dir, store, owner, c) = fixture().await;
    let p = InviteRequest {
        email: "guest@partner.test".into(),
        role: AccessRole::Editor,
    };
    assert!(
        owner
            .prepare_invitation(&c.id, p.clone(), &["example.com".into()])
            .await
            .is_err()
    );
    Workspace::new(store.clone(), local_user())
        .set_policy(OrganizationPolicy {
            canvas_enabled: true,
            external_invites: true,
        })
        .await
        .unwrap();
    let (summary, token, _) = owner
        .prepare_invitation(&c.id, p, &["example.com".into()])
        .await
        .unwrap();
    let guest = Workspace::new(
        store.clone(),
        UserInfo {
            internal: false,
            ..person("guest", "guest@partner.test")
        },
    );
    assert!(guest.accept_invitation(&token).await.is_err());
    owner
        .invitation_delivered(&c.id, &summary.id, true)
        .await
        .unwrap();
    assert!(
        Workspace::new(store.clone(), person("wrong", "other@partner.test"))
            .accept_invitation(&token)
            .await
            .is_err()
    );
    assert_eq!(guest.accept_invitation(&token).await.unwrap(), c.id);
    assert_eq!(guest.role(&c.id).await.unwrap(), AccessRole::Editor);
    assert!(
        Workspace::new(store.clone(), person("new-subject", "guest@partner.test"))
            .accept_invitation(&token)
            .await
            .is_err()
    );
    owner.revoke_member(&c.id, "guest").await.unwrap();
    assert!(guest.accept_invitation(&token).await.is_err());
    let (summary, expired, _) = owner
        .prepare_invitation(
            &c.id,
            InviteRequest {
                email: "expired@example.com".into(),
                role: AccessRole::Viewer,
            },
            &["example.com".into()],
        )
        .await
        .unwrap();
    owner
        .invitation_delivered(&c.id, &summary.id, true)
        .await
        .unwrap();
    let mut row = InvitationRecord::filter_by_id(&summary.id)
        .get(&mut store.db.clone())
        .await
        .unwrap();
    toasty::update!(row {
        expires_at: timestamp() - 1
    })
    .exec(&mut store.db.clone())
    .await
    .unwrap();
    assert!(
        Workspace::new(store, person("expired", "expired@example.com"))
            .accept_invitation(&expired)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn commenters_cannot_edit_and_viewers_cannot_comment() {
    let (_dir, store, _owner, c) = fixture().await;
    grant(&store, &c.id, "reviewer", AccessRole::Commenter).await;
    grant(&store, &c.id, "viewer", AccessRole::Viewer).await;
    let reviewer = Workspace::new(store.clone(), person("reviewer", "reviewer@example.com"));
    let viewer = Workspace::new(store, person("viewer", "viewer@example.com"));
    let input = NewThread {
        anchor_id: c.nodes[0].id.clone(),
        x: 0.,
        y: 0.,
        body: "Please review this".into(),
    };
    assert!(viewer.new_thread(&c.id, input.clone()).await.is_err());
    let thread = reviewer.new_thread(&c.id, input).await.unwrap();
    assert_eq!(thread.replies[0].author_id, "reviewer");
    assert!(
        reviewer
            .edit(
                &c.id,
                &Mutation {
                    base_revision: 1,
                    scope: None,
                    operations: vec![Operation::Rename { title: "No".into() }]
                }
            )
            .await
            .is_err()
    );
    assert!(viewer.read(&c.id).await.is_ok());
    assert!(viewer.sharing(&c.id).await.is_err());
}
#[tokio::test]
async fn replies_are_not_lost_and_deleted_anchors_keep_their_threads() {
    let (dir, store, owner, c) = fixture().await;
    let thread = owner
        .new_thread(
            &c.id,
            NewThread {
                anchor_id: c.nodes[0].id.clone(),
                x: 0.,
                y: 0.,
                body: "Discuss this".into(),
            },
        )
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        owner.reply(
            &c.id,
            &thread.id,
            Reply {
                body: "First".into()
            }
        ),
        owner.reply(
            &c.id,
            &thread.id,
            Reply {
                body: "Second".into()
            }
        )
    );
    a.unwrap();
    b.unwrap();
    assert_eq!(owner.comments(&c.id).await.unwrap()[0].replies.len(), 3);
    let current = owner.read(&c.id).await.unwrap();
    owner
        .edit(
            &c.id,
            &Mutation {
                base_revision: current.revision,
                scope: None,
                operations: vec![Operation::DeleteNode {
                    id: c.nodes[0].id.clone(),
                }],
            },
        )
        .await
        .unwrap();
    let saved = owner.comments(&c.id).await.unwrap().remove(0);
    assert!(saved.anchor_id.is_empty());
    assert_eq!(saved.anchor_title, "Review");
    assert_eq!(saved.x, 320.);
    assert_eq!(saved.y, 100.);
    owner
        .resolve(&c.id, &thread.id, ResolveThread { resolved: true })
        .await
        .unwrap();
    drop(owner);
    drop(store);
    let reopened = Store::open(&dir.path().join("team.db"), None, false)
        .await
        .unwrap();
    assert!(
        Workspace::new(reopened, person("owner", "owner@example.com"))
            .comments(&c.id)
            .await
            .unwrap()[0]
            .resolved
    );
}
#[tokio::test]
async fn organization_policy_is_admin_only_and_blocks_all_canvas_access() {
    let (_dir, store, owner, c) = fixture().await;
    assert!(
        owner
            .set_policy(OrganizationPolicy {
                canvas_enabled: false,
                external_invites: true
            })
            .await
            .is_err()
    );
    let admin = Workspace::new(store.clone(), local_user());
    admin
        .set_policy(OrganizationPolicy {
            canvas_enabled: false,
            external_invites: false,
        })
        .await
        .unwrap();
    assert!(owner.read(&c.id).await.is_err());
    assert!(admin.read(&c.id).await.is_err());
    assert!(owner.create("Blocked", vec![], vec![]).await.is_err());
    admin
        .set_policy(OrganizationPolicy::default())
        .await
        .unwrap();
    assert!(owner.read(&c.id).await.is_ok());
}
