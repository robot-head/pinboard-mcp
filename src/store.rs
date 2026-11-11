//! Typed Toasty persistence over the Turso driver. No application SQL queries.
use crate::models::{
    CanvasRecord, CommentRecord, GrantRecord, IdentityRecord, InvitationRecord, Metadata,
    MutationRecord, SessionRecord, UserRecord,
};
use pinboard_core::{Canvas, CanvasError, CanvasSummary, Mutation, welcome_canvas};
use std::path::Path;
use toasty::Db;
use toasty_core::driver::operation::TransactionMode;
use toasty_driver_turso::Turso;
use tokio::sync::broadcast;

#[derive(Clone, Debug, serde::Serialize)]
pub struct Change {
    pub canvas_id: String,
    pub revision: u64,
}
#[derive(Debug)]
pub enum StoreError {
    Canvas(CanvasError),
    Internal(anyhow::Error),
    Forbidden(String),
    Unauthorized,
    Unavailable(String),
}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Canvas(e) => write!(f, "{e}"),
            Self::Internal(e) => write!(f, "{e}"),
            Self::Forbidden(e) | Self::Unavailable(e) => write!(f, "{e}"),
            Self::Unauthorized => write!(f, "Sign in is required"),
        }
    }
}
impl std::error::Error for StoreError {}
impl From<CanvasError> for StoreError {
    fn from(e: CanvasError) -> Self {
        Self::Canvas(e)
    }
}
impl From<toasty::Error> for StoreError {
    fn from(e: toasty::Error) -> Self {
        Self::Internal(e.into())
    }
}
impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self {
        Self::Internal(e.into())
    }
}
pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Clone)]
pub struct Store {
    pub(crate) db: Db,
    pub(crate) write_lock: std::sync::Arc<tokio::sync::Mutex<()>>,
    pub changes: broadcast::Sender<Change>,
}
impl Store {
    pub async fn open(
        path: &Path,
        remote: Option<(String, String)>,
        seed: bool,
    ) -> anyhow::Result<Self> {
        let driver = if let Some((url, token)) = remote {
            anyhow::ensure!(
                url.starts_with("turso://")
                    || url.starts_with("libsql://")
                    || url.starts_with("https://"),
                "Use a turso://, libsql://, or https:// Turso Cloud URL"
            );
            Turso::new(url)?.with_auth_token(token)
        } else {
            if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            Turso::file(path)
        };
        let mut db = Db::builder()
            .models(toasty::models!(
                CanvasRecord,
                MutationRecord,
                Metadata,
                UserRecord,
                SessionRecord,
                GrantRecord,
                InvitationRecord,
                CommentRecord,
                IdentityRecord
            ))
            .max_pool_size(4)
            .build(driver)
            .await?;
        const MIGRATIONS: toasty::migration::MigrationSet =
            toasty::migration::MigrationSet::new(&[
                toasty::migration::MigrationFile::new(
                    1,
                    "initial_canvas_models",
                    include_str!("../migrations/0001_canvas_models.sql"),
                ),
                toasty::migration::MigrationFile::new(
                    2,
                    "collaboration_models",
                    include_str!("../migrations/0002_collaboration.sql"),
                ),
            ]);
        MIGRATIONS.apply(&db).await?;
        if seed {
            let mut tx = db
                .transaction_builder()
                .mode(TransactionMode::Immediate)
                .begin()
                .await?;
            if Metadata::filter_by_id("seeded")
                .first()
                .exec(&mut tx)
                .await?
                .is_none()
            {
                let c = welcome_canvas();
                toasty::create!(CanvasRecord {
                    id: c.id.clone(),
                    document: serde_json::to_string(&c)?,
                    updated_at: c.updated_at.clone()
                })
                .exec(&mut tx)
                .await?;
                toasty::create!(Metadata {
                    id: "seeded",
                    value: "true"
                })
                .exec(&mut tx)
                .await?;
            }
            tx.commit().await?;
        }
        let (changes, _) = broadcast::channel(256);
        Ok(Self {
            db,
            write_lock: Default::default(),
            changes,
        })
    }
    pub async fn list(&self) -> Result<Vec<CanvasSummary>> {
        let records = CanvasRecord::all().exec(&mut self.db.clone()).await?;
        let mut summaries = records
            .into_iter()
            .map(|r| Ok(serde_json::from_str::<Canvas>(&r.document)?.summary()))
            .collect::<Result<Vec<_>>>()?;
        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }
    pub async fn get(&self, id: &str) -> Result<Canvas> {
        let record = CanvasRecord::filter_by_id(id)
            .first()
            .exec(&mut self.db.clone())
            .await?
            .ok_or_else(|| CanvasError::NotFound(id.into()))?;
        Ok(serde_json::from_str(&record.document)?)
    }
    #[cfg(test)]
    pub async fn create(
        &self,
        title: &str,
        nodes: Vec<pinboard_core::Node>,
        edges: Vec<pinboard_core::Edge>,
    ) -> Result<Canvas> {
        let _writer = self.write_lock.lock().await;
        let mut c = Canvas::new(title);
        c.nodes = nodes;
        c.edges = edges;
        c.validate()?;
        toasty::create!(CanvasRecord {
            id: c.id.clone(),
            document: serde_json::to_string(&c)?,
            updated_at: c.updated_at.clone()
        })
        .exec(&mut self.db.clone())
        .await?;
        Ok(c)
    }
    #[cfg(test)]
    pub async fn mutate(&self, id: &str, batch: &Mutation) -> Result<Canvas> {
        self.mutate_as(&crate::collab::local_user(), id, batch)
            .await
    }
    pub async fn mutate_as(
        &self,
        user: &pinboard_core::UserInfo,
        id: &str,
        batch: &Mutation,
    ) -> Result<Canvas> {
        let _writer = self.write_lock.lock().await;
        let mut db = self.db.clone();
        // Lock before reading so independent remote clients cannot both write
        // using the same revision. State and history commit in one transaction.
        let mut tx = db
            .transaction_builder()
            .mode(TransactionMode::Immediate)
            .begin()
            .await?;
        let result: Result<Canvas> = async {
            crate::collab::authorize_write(&mut tx, user, id, crate::collab::WritePermission::Edit)
                .await?;
            let mut record = CanvasRecord::filter_by_id(id)
                .first()
                .exec(&mut tx)
                .await?
                .ok_or_else(|| CanvasError::NotFound(id.into()))?;
            let mut c: Canvas = serde_json::from_str(&record.document)?;
            let before = c.clone();
            c.apply(batch)?;
            let comments = CommentRecord::filter_by_canvas_id(id).exec(&mut tx).await?;
            for mut comment in comments {
                let mut thread: pinboard_core::CommentThread =
                    serde_json::from_str(&comment.document)?;
                if !thread.anchor_id.is_empty() && !c.nodes.iter().any(|n| n.id == thread.anchor_id)
                {
                    if let Some(anchor) = before.nodes.iter().find(|n| n.id == thread.anchor_id) {
                        thread.x = anchor.x + anchor.width;
                        thread.y = anchor.y;
                        thread.anchor_title = anchor.title.clone();
                    }
                    thread.anchor_id.clear();
                    thread.updated_at = pinboard_core::now();
                    toasty::update!(comment {
                        document: serde_json::to_string(&thread)?
                    })
                    .exec(&mut tx)
                    .await?;
                }
            }
            toasty::update!(record {
                document: serde_json::to_string(&c)?,
                updated_at: c.updated_at.clone()
            })
            .exec(&mut tx)
            .await?;
            toasty::create!(MutationRecord {
                id: format!("{}:{}", id, c.revision),
                canvas_id: id,
                revision: c.revision as i64,
                body: serde_json::to_string(batch)?,
                created_at: c.updated_at.clone()
            })
            .exec(&mut tx)
            .await?;
            Ok(c)
        }
        .await;
        match result {
            Ok(c) => {
                tx.commit().await?;
                let _ = self.changes.send(Change {
                    canvas_id: id.into(),
                    revision: c.revision,
                });
                Ok(c)
            }
            Err(e) => {
                tx.rollback().await?;
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pinboard_core::{Node, NodePatch, Operation};
    #[tokio::test]
    async fn turso_persists_and_tracks_migrations_on_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(&path, None, false).await.unwrap();
        let c = store
            .create("Durable", vec![Node::note("A", 0., 0.)], vec![])
            .await
            .unwrap();
        let changed = store
            .mutate(
                &c.id,
                &Mutation {
                    base_revision: 0,
                    scope: None,
                    operations: vec![Operation::Rename {
                        title: "Saved".into(),
                    }],
                },
            )
            .await
            .unwrap();
        assert_eq!(changed.revision, 1);
        let history = MutationRecord::filter_by_canvas_id(&c.id)
            .exec(&mut store.db.clone())
            .await
            .unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].revision, 1);
        drop(store);
        let reopened = Store::open(&path, None, false).await.unwrap();
        assert_eq!(reopened.get(&c.id).await.unwrap().title, "Saved");
        assert_eq!(reopened.list().await.unwrap().len(), 1);
    }
    #[tokio::test]
    async fn concurrent_writers_cannot_both_commit_the_same_revision() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("race.db"), None, false)
            .await
            .unwrap();
        let c = store.create("Race", vec![], vec![]).await.unwrap();
        let a = Mutation {
            base_revision: 0,
            scope: None,
            operations: vec![Operation::Rename { title: "A".into() }],
        };
        let b = Mutation {
            base_revision: 0,
            scope: None,
            operations: vec![Operation::Rename { title: "B".into() }],
        };
        let (a, b) = tokio::join!(store.mutate(&c.id, &a), store.mutate(&c.id, &b));
        assert_ne!(a.is_ok(), b.is_ok());
        let error = a.err().or_else(|| b.err());
        assert!(
            matches!(
                error,
                Some(StoreError::Canvas(CanvasError::Conflict { .. }))
            ),
            "{error:?}"
        );
        assert_eq!(store.get(&c.id).await.unwrap().revision, 1);
    }
    #[tokio::test]
    async fn invalid_batch_writes_neither_document_nor_history() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("rollback.db"), None, false)
            .await
            .unwrap();
        let c = store
            .create("Original", vec![Node::note("A", 0., 0.)], vec![])
            .await
            .unwrap();
        let result = store
            .mutate(
                &c.id,
                &Mutation {
                    base_revision: 0,
                    scope: None,
                    operations: vec![
                        Operation::Rename {
                            title: "Partial".into(),
                        },
                        Operation::UpdateNode {
                            id: c.nodes[0].id.clone(),
                            changes: NodePatch {
                                width: Some(-10.),
                                ..Default::default()
                            },
                        },
                    ],
                },
            )
            .await;
        assert!(result.is_err());
        let unchanged = store.get(&c.id).await.unwrap();
        assert_eq!(unchanged.title, "Original");
        assert_eq!(unchanged.revision, 0);
        assert!(
            MutationRecord::all()
                .exec(&mut store.db.clone())
                .await
                .unwrap()
                .is_empty()
        );
        store
            .mutate(
                &c.id,
                &Mutation {
                    base_revision: 0,
                    scope: None,
                    operations: vec![Operation::Rename {
                        title: "Valid after rollback".into(),
                    }],
                },
            )
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn welcome_board_is_only_seeded_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("demo.db");
        let first = Store::open(&path, None, true).await.unwrap();
        assert_eq!(first.list().await.unwrap().len(), 1);
        drop(first);
        let second = Store::open(&path, None, true).await.unwrap();
        assert_eq!(second.list().await.unwrap().len(), 1);
    }
}
