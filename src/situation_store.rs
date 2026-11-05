//! Durable bounded Situation state for visible reanalysis.
//!
//! This storage layer preserves human contributions, incremental sources and
//! immutable revision snapshots. Human text is attributed context, not promoted
//! to verified evidence. The unresolved semantics of remembered hypothesis
//! rejection are deliberately outside this module.
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum SituationStoreError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid situation input")]
    Invalid,
    #[error("immutable identifier replay conflicts with stored state")]
    Conflict,
    #[error("situation or referenced source is not visible")]
    Invisible,
    #[error("contribution is no longer active")]
    Inactive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContributionKind {
    UserReported,
    Hypothetical,
    InvestigativePreference,
}
impl ContributionKind {
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "user_reported" => Some(Self::UserReported),
            "hypothetical" => Some(Self::Hypothetical),
            "investigative_preference" => Some(Self::InvestigativePreference),
            _ => None,
        }
    }
    fn db_value(self) -> &'static str {
        match self {
            Self::UserReported => "USER_REPORTED",
            Self::Hypothetical => "HYPOTHETICAL",
            Self::InvestigativePreference => "INVESTIGATIVE_PREFERENCE",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotContribution {
    pub id: Uuid,
    pub author: Uuid,
    pub kind: String,
    pub text: String,
    pub scope: String,
    pub related_evidence_id: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SituationSnapshot {
    pub situation_id: Uuid,
    pub revision: i64,
    pub fingerprint: String,
    pub focus: String,
    pub sources: Vec<Uuid>,
    pub contributions: Vec<SnapshotContribution>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionSummary {
    pub revision: i64,
    pub fingerprint: String,
    pub focus: String,
    pub actor: Uuid,
    pub cause: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MutationResult {
    pub snapshot: SituationSnapshot,
    pub replayed: bool,
}

type ActiveContributionRow = (Uuid, Uuid, String, String, String, Option<Uuid>);

fn clean_text(input: &str) -> Result<String, SituationStoreError> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.len() > 4096 || trimmed.chars().any(char::is_control) {
        return Err(SituationStoreError::Invalid);
    }
    Ok(trimmed.to_owned())
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, SituationStoreError> {
    let mut tx = pool.begin().await?;
    if read_only {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
    }
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(SituationStoreError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

async fn visible_situation(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2)",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut **tx)
    .await
}

async fn current_snapshot_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<SituationSnapshot, SituationStoreError> {
    let revision: Option<i64> = sqlx::query_scalar(
        "SELECT current_revision FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(revision) = revision else {
        return Err(SituationStoreError::Invisible);
    };
    if revision < 1 {
        return Err(SituationStoreError::Conflict);
    }
    snapshot_in_tx(tx, workspace, situation, revision).await
}

async fn snapshot_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    revision: i64,
) -> Result<SituationSnapshot, SituationStoreError> {
    let header: Option<(String, String)> = sqlx::query_as(
        "SELECT focus,fingerprint FROM niccolo.situation_revisions \
         WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((focus, fingerprint)) = header else {
        return Err(SituationStoreError::Invisible);
    };
    let sources: Vec<Uuid> = sqlx::query_scalar(
        "SELECT document_id FROM niccolo.situation_revision_sources \
         WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3 ORDER BY document_id",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .fetch_all(&mut **tx)
    .await?;
    let rows: Vec<ActiveContributionRow> = sqlx::query_as(
        "SELECT c.contribution_id,c.author_principal_id,c.kind,c.contribution_text,c.scope,c.related_evidence_id \
         FROM niccolo.situation_revision_contributions rc \
         JOIN niccolo.situation_contributions c USING(workspace_id,situation_id,contribution_id) \
         WHERE rc.workspace_id=$1 AND rc.situation_id=$2 AND rc.revision=$3 \
         ORDER BY c.contribution_id",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .fetch_all(&mut **tx)
    .await?;
    let contributions = rows
        .into_iter()
        .map(
            |(id, author, kind, text, scope, related_evidence_id)| SnapshotContribution {
                id,
                author,
                kind,
                text,
                scope,
                related_evidence_id,
            },
        )
        .collect();
    Ok(SituationSnapshot {
        situation_id: situation,
        revision,
        fingerprint,
        focus,
        sources,
        contributions,
    })
}

async fn checkpoint(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    cause: &'static str,
) -> Result<SituationSnapshot, SituationStoreError> {
    let row: Option<(String, i64)> = sqlx::query_as(
        "SELECT focus,current_revision FROM niccolo.situations \
         WHERE workspace_id=$1 AND situation_id=$2 FOR UPDATE",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((focus, previous_revision)) = row else {
        return Err(SituationStoreError::Invisible);
    };
    let sources: Vec<Uuid> = sqlx::query_scalar(
        "SELECT document_id FROM niccolo.situation_sources \
         WHERE workspace_id=$1 AND situation_id=$2 ORDER BY document_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut **tx)
    .await?;
    let active: Vec<ActiveContributionRow> = sqlx::query_as(
        "SELECT contribution_id,author_principal_id,kind,contribution_text,scope,related_evidence_id \
         FROM niccolo.situation_contributions \
         WHERE workspace_id=$1 AND situation_id=$2 AND status='ACTIVE' \
         ORDER BY contribution_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut **tx)
    .await?;
    let fingerprint = fingerprint(&focus, &sources, &active);
    let revision = previous_revision + 1;
    sqlx::query(
        "INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id,cause) \
         VALUES($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .bind(&focus)
    .bind(&fingerprint)
    .bind(actor)
    .bind(cause)
    .execute(&mut **tx)
    .await?;
    for document in &sources {
        sqlx::query(
            "INSERT INTO niccolo.situation_revision_sources(workspace_id,situation_id,revision,document_id) VALUES($1,$2,$3,$4)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(revision)
        .bind(document)
        .execute(&mut **tx)
        .await?;
    }
    for (id, ..) in &active {
        sqlx::query(
            "INSERT INTO niccolo.situation_revision_contributions(workspace_id,situation_id,revision,contribution_id) VALUES($1,$2,$3,$4)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(revision)
        .bind(id)
        .execute(&mut **tx)
        .await?;
    }
    sqlx::query(
        "UPDATE niccolo.situations SET current_revision=$3,updated_at=clock_timestamp() \
         WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(workspace)
    .bind(situation)
    .bind(revision)
    .execute(&mut **tx)
    .await?;
    Ok(SituationSnapshot {
        situation_id: situation,
        revision,
        fingerprint,
        focus,
        sources,
        contributions: active
            .into_iter()
            .map(
                |(id, author, kind, text, scope, related_evidence_id)| SnapshotContribution {
                    id,
                    author,
                    kind,
                    text,
                    scope,
                    related_evidence_id,
                },
            )
            .collect(),
    })
}

fn fingerprint(focus: &str, sources: &[Uuid], contributions: &[ActiveContributionRow]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"niccolo-situation-pg-v1\0");
    digest.update(focus.as_bytes());
    digest.update(b"\0sources\0");
    for id in sources {
        digest.update(id.as_bytes());
    }
    digest.update(b"\0contributions\0");
    for (id, author, kind, text, scope, evidence) in contributions {
        digest.update(id.as_bytes());
        digest.update(author.as_bytes());
        digest.update(kind.as_bytes());
        digest.update(b"\0");
        digest.update(text.as_bytes());
        digest.update(b"\0");
        digest.update(scope.as_bytes());
        digest.update(b"\0");
        if let Some(evidence) = evidence {
            digest.update(evidence.as_bytes());
        }
        digest.update(b"\0");
    }
    format!("{:x}", digest.finalize())
}

pub async fn create(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    situation: Uuid,
    focus: &str,
) -> Result<MutationResult, SituationStoreError> {
    let focus = clean_text(focus)?;
    let mut tx = scoped(pool, workspace, false).await?;
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id) \
         VALUES($1,$2,$3,$4) ON CONFLICT(workspace_id,situation_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(&focus)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if inserted {
        let snapshot = checkpoint(&mut tx, workspace, situation, actor, "CREATED").await?;
        tx.commit().await?;
        return Ok(MutationResult {
            snapshot,
            replayed: false,
        });
    }
    let stored: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT focus,created_by_principal_id FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_optional(&mut *tx)
    .await?;
    if stored != Some((focus, actor)) {
        return Err(SituationStoreError::Conflict);
    }
    let snapshot = current_snapshot_in_tx(&mut tx, workspace, situation).await?;
    tx.commit().await?;
    Ok(MutationResult {
        snapshot,
        replayed: true,
    })
}

pub async fn set_focus(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    situation: Uuid,
    focus: &str,
) -> Result<MutationResult, SituationStoreError> {
    let focus = clean_text(focus)?;
    let mut tx = scoped(pool, workspace, false).await?;
    let current: Option<String> = sqlx::query_scalar(
        "SELECT focus FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2 FOR UPDATE",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(current) = current else {
        return Err(SituationStoreError::Invisible);
    };
    if current == focus {
        let snapshot = current_snapshot_in_tx(&mut tx, workspace, situation).await?;
        tx.commit().await?;
        return Ok(MutationResult {
            snapshot,
            replayed: true,
        });
    }
    sqlx::query("UPDATE niccolo.situations SET focus=$3 WHERE workspace_id=$1 AND situation_id=$2")
        .bind(workspace)
        .bind(situation)
        .bind(focus)
        .execute(&mut *tx)
        .await?;
    let snapshot = checkpoint(&mut tx, workspace, situation, actor, "FOCUS_CHANGED").await?;
    tx.commit().await?;
    Ok(MutationResult {
        snapshot,
        replayed: false,
    })
}

pub async fn add_source(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    situation: Uuid,
    document: Uuid,
) -> Result<MutationResult, SituationStoreError> {
    let mut tx = scoped(pool, workspace, false).await?;
    if !visible_situation(&mut tx, workspace, situation).await? {
        return Err(SituationStoreError::Invisible);
    }
    let source_visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.source_documents WHERE workspace_id=$1 AND document_id=$2)",
    )
    .bind(workspace)
    .bind(document)
    .fetch_one(&mut *tx)
    .await?;
    if !source_visible {
        return Err(SituationStoreError::Invisible);
    }
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_sources(workspace_id,situation_id,document_id,added_by_principal_id) \
         VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(document)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let snapshot = if inserted {
        checkpoint(&mut tx, workspace, situation, actor, "SOURCE_ADDED").await?
    } else {
        current_snapshot_in_tx(&mut tx, workspace, situation).await?
    };
    tx.commit().await?;
    Ok(MutationResult {
        snapshot,
        replayed: !inserted,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "attribution and optional source relationship are explicit audit inputs"
)]
pub async fn add_contribution(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    situation: Uuid,
    contribution: Uuid,
    kind: ContributionKind,
    text: &str,
    related_evidence_id: Option<Uuid>,
) -> Result<MutationResult, SituationStoreError> {
    let text = clean_text(text)?;
    let mut tx = scoped(pool, workspace, false).await?;
    if !visible_situation(&mut tx, workspace, situation).await? {
        return Err(SituationStoreError::Invisible);
    }
    if let Some(evidence) = related_evidence_id {
        let visible: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.evidence WHERE workspace_id=$1 AND evidence_id=$2)",
        )
        .bind(workspace)
        .bind(evidence)
        .fetch_one(&mut *tx)
        .await?;
        if !visible {
            return Err(SituationStoreError::Invisible);
        }
    }
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_contributions(workspace_id,situation_id,contribution_id,author_principal_id,kind,contribution_text,related_evidence_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(workspace_id,situation_id,contribution_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(contribution)
    .bind(actor)
    .bind(kind.db_value())
    .bind(&text)
    .bind(related_evidence_id)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !inserted {
        let stored: Option<(Uuid, String, String, Option<Uuid>)> = sqlx::query_as(
            "SELECT author_principal_id,kind,contribution_text,related_evidence_id \
             FROM niccolo.situation_contributions WHERE workspace_id=$1 AND situation_id=$2 AND contribution_id=$3",
        )
        .bind(workspace)
        .bind(situation)
        .bind(contribution)
        .fetch_optional(&mut *tx)
        .await?;
        if stored != Some((actor, kind.db_value().to_owned(), text, related_evidence_id)) {
            return Err(SituationStoreError::Conflict);
        }
        let snapshot = current_snapshot_in_tx(&mut tx, workspace, situation).await?;
        tx.commit().await?;
        return Ok(MutationResult {
            snapshot,
            replayed: true,
        });
    }
    let snapshot = checkpoint(&mut tx, workspace, situation, actor, "CONTRIBUTION_ADDED").await?;
    tx.commit().await?;
    Ok(MutationResult {
        snapshot,
        replayed: false,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "correction keeps old/new identity, attribution and source relation explicit"
)]
pub async fn correct_contribution(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    situation: Uuid,
    old: Uuid,
    replacement: Uuid,
    text: &str,
    related_evidence_id: Option<Uuid>,
) -> Result<MutationResult, SituationStoreError> {
    if old == replacement {
        return Err(SituationStoreError::Invalid);
    }
    let text = clean_text(text)?;
    let mut tx = scoped(pool, workspace, false).await?;
    let original: Option<(String, String, Option<Uuid>)> = sqlx::query_as(
        "SELECT kind,status,corrected_by FROM niccolo.situation_contributions \
         WHERE workspace_id=$1 AND situation_id=$2 AND contribution_id=$3 FOR UPDATE",
    )
    .bind(workspace)
    .bind(situation)
    .bind(old)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((kind, status, corrected_by)) = original else {
        return Err(SituationStoreError::Invisible);
    };
    if let Some(evidence) = related_evidence_id {
        let visible: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.evidence WHERE workspace_id=$1 AND evidence_id=$2)",
        )
        .bind(workspace)
        .bind(evidence)
        .fetch_one(&mut *tx)
        .await?;
        if !visible {
            return Err(SituationStoreError::Invisible);
        }
    }
    if status == "CORRECTED" && corrected_by == Some(replacement) {
        let stored: Option<(Uuid, String, String, Option<Uuid>)> = sqlx::query_as(
            "SELECT author_principal_id,kind,contribution_text,related_evidence_id \
             FROM niccolo.situation_contributions WHERE workspace_id=$1 AND situation_id=$2 AND contribution_id=$3",
        )
        .bind(workspace)
        .bind(situation)
        .bind(replacement)
        .fetch_optional(&mut *tx)
        .await?;
        if stored != Some((actor, kind, text, related_evidence_id)) {
            return Err(SituationStoreError::Conflict);
        }
        let snapshot = current_snapshot_in_tx(&mut tx, workspace, situation).await?;
        tx.commit().await?;
        return Ok(MutationResult {
            snapshot,
            replayed: true,
        });
    }
    if status != "ACTIVE" {
        return Err(SituationStoreError::Inactive);
    }
    let inserted = sqlx::query(
        "INSERT INTO niccolo.situation_contributions(workspace_id,situation_id,contribution_id,author_principal_id,kind,contribution_text,related_evidence_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(workspace_id,situation_id,contribution_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(replacement)
    .bind(actor)
    .bind(&kind)
    .bind(&text)
    .bind(related_evidence_id)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !inserted {
        return Err(SituationStoreError::Conflict);
    }
    sqlx::query(
        "UPDATE niccolo.situation_contributions SET status='CORRECTED',corrected_by=$4,changed_at=clock_timestamp() \
         WHERE workspace_id=$1 AND situation_id=$2 AND contribution_id=$3 AND status='ACTIVE'",
    )
    .bind(workspace)
    .bind(situation)
    .bind(old)
    .bind(replacement)
    .execute(&mut *tx)
    .await?;
    let snapshot = checkpoint(&mut tx, workspace, situation, actor, "CORRECTED").await?;
    tx.commit().await?;
    Ok(MutationResult {
        snapshot,
        replayed: false,
    })
}

pub async fn retract_contribution(
    pool: &PgPool,
    workspace: Uuid,
    actor: Uuid,
    situation: Uuid,
    contribution: Uuid,
) -> Result<MutationResult, SituationStoreError> {
    let mut tx = scoped(pool, workspace, false).await?;
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM niccolo.situation_contributions \
         WHERE workspace_id=$1 AND situation_id=$2 AND contribution_id=$3 FOR UPDATE",
    )
    .bind(workspace)
    .bind(situation)
    .bind(contribution)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(status) = status else {
        return Err(SituationStoreError::Invisible);
    };
    if status == "RETRACTED" {
        let snapshot = current_snapshot_in_tx(&mut tx, workspace, situation).await?;
        tx.commit().await?;
        return Ok(MutationResult {
            snapshot,
            replayed: true,
        });
    }
    if status != "ACTIVE" {
        return Err(SituationStoreError::Inactive);
    }
    sqlx::query(
        "UPDATE niccolo.situation_contributions SET status='RETRACTED',changed_at=clock_timestamp() \
         WHERE workspace_id=$1 AND situation_id=$2 AND contribution_id=$3 AND status='ACTIVE'",
    )
    .bind(workspace)
    .bind(situation)
    .bind(contribution)
    .execute(&mut *tx)
    .await?;
    let snapshot = checkpoint(&mut tx, workspace, situation, actor, "RETRACTED").await?;
    tx.commit().await?;
    Ok(MutationResult {
        snapshot,
        replayed: false,
    })
}

pub async fn get_snapshot(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    revision: Option<i64>,
) -> Result<SituationSnapshot, SituationStoreError> {
    if revision.is_some_and(|r| r < 1) {
        return Err(SituationStoreError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let snapshot = match revision {
        Some(revision) => snapshot_in_tx(&mut tx, workspace, situation, revision).await?,
        None => current_snapshot_in_tx(&mut tx, workspace, situation).await?,
    };
    tx.commit().await?;
    Ok(snapshot)
}

pub async fn list_revisions(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    after: i64,
    limit: i64,
) -> Result<(Vec<RevisionSummary>, Option<i64>), SituationStoreError> {
    if after < 0 || !(1..=100).contains(&limit) {
        return Err(SituationStoreError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    if !visible_situation(&mut tx, workspace, situation).await? {
        return Err(SituationStoreError::Invisible);
    }
    let rows: Vec<(i64, String, String, Uuid, String)> = sqlx::query_as(
        "SELECT revision,fingerprint,focus,actor_principal_id,cause \
         FROM niccolo.situation_revisions WHERE workspace_id=$1 AND situation_id=$2 AND revision>$3 \
         ORDER BY revision LIMIT $4",
    )
    .bind(workspace)
    .bind(situation)
    .bind(after)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let more = rows.len() as i64 > limit;
    let revisions = rows
        .into_iter()
        .take(limit as usize)
        .map(
            |(revision, fingerprint, focus, actor, cause)| RevisionSummary {
                revision,
                fingerprint,
                focus,
                actor,
                cause,
            },
        )
        .collect::<Vec<_>>();
    let next = if more {
        revisions.last().map(|r| r.revision)
    } else {
        None
    };
    Ok((revisions, next))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contribution_kind_is_closed_and_hypothetical_is_distinct() {
        assert_eq!(
            ContributionKind::parse("hypothetical"),
            Some(ContributionKind::Hypothetical)
        );
        assert_eq!(
            ContributionKind::parse("user_reported"),
            Some(ContributionKind::UserReported)
        );
        assert_eq!(ContributionKind::parse("evidence"), None);
        assert_ne!(
            ContributionKind::Hypothetical.db_value(),
            ContributionKind::UserReported.db_value()
        );
    }

    #[test]
    fn fingerprint_changes_for_actor_kind_text_source_or_document() {
        let doc = Uuid::from_u128(1);
        let id = Uuid::from_u128(2);
        let actor = Uuid::from_u128(3);
        let base = vec![(
            id,
            actor,
            "USER_REPORTED".to_owned(),
            "context".to_owned(),
            "SITUATION".to_owned(),
            None,
        )];
        let f = fingerprint("focus", &[doc], &base);
        assert_eq!(f.len(), 64);
        let mut changed = base.clone();
        changed[0].2 = "HYPOTHETICAL".to_owned();
        assert_ne!(f, fingerprint("focus", &[doc], &changed));
        assert_ne!(f, fingerprint("other focus", &[doc], &base));
        assert_ne!(f, fingerprint("focus", &[doc, Uuid::from_u128(4)], &base));
    }
}
