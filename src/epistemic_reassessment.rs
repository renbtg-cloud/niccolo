//! Selective reassessment over explicit expert-authored grounding.
//!
//! A changed ground identifies downstream arguments that require review. This module
//! deliberately does not convert "needs reassessment" into falsity, defeat or automatic
//! replacement of a conclusion: independent grounding may still survive.
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ReassessmentError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid reassessment request")]
    Invalid,
    #[error("epistemic object is not visible in this Situation")]
    Invisible,
    #[error("reassessment replay conflict")]
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReassessmentChangeKind {
    Revised,
    Retracted,
    Superseded,
    Reinterpreted,
    ManualReview,
}
impl ReassessmentChangeKind {
    fn db_value(self) -> &'static str {
        match self {
            Self::Revised => "REVISED",
            Self::Retracted => "RETRACTED",
            Self::Superseded => "SUPERSEDED",
            Self::Reinterpreted => "REINTERPRETED",
            Self::ManualReview => "MANUAL_REVIEW",
        }
    }
    fn from_db(value: &str) -> Result<Self, ReassessmentError> {
        match value {
            "REVISED" => Ok(Self::Revised),
            "RETRACTED" => Ok(Self::Retracted),
            "SUPERSEDED" => Ok(Self::Superseded),
            "REINTERPRETED" => Ok(Self::Reinterpreted),
            "MANUAL_REVIEW" => Ok(Self::ManualReview),
            _ => Err(ReassessmentError::Invalid),
        }
    }
    fn requires_replacement(self) -> bool {
        matches!(self, Self::Revised | Self::Superseded | Self::Reinterpreted)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReassessmentImpact {
    pub argument_id: Uuid,
    pub conclusion_object_id: Uuid,
    pub minimum_depth: u16,
    pub impact_scope: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReassessmentEventView {
    pub event_id: Uuid,
    pub situation_id: Uuid,
    pub trigger_object_id: Uuid,
    pub replacement_object_id: Option<Uuid>,
    pub change_kind: ReassessmentChangeKind,
    pub rationale: String,
    pub recorded_by: Uuid,
    pub impacts: Vec<ReassessmentImpact>,
    pub automatic_conclusion_status_change: bool,
    pub replayed: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, ReassessmentError> {
    let mut tx = pool.begin().await?;
    sqlx::query(if read_only {
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
    } else {
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
    })
    .execute(&mut *tx)
    .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ReassessmentError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

fn valid_text(value: &str, max: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && value.len() <= max && !value.contains('\0')
}

async fn object_visible_in_situation(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    object_id: Uuid,
) -> Result<bool, ReassessmentError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS( \
           SELECT 1 FROM niccolo.epistemic_propositions p \
            WHERE p.workspace_id=$1 AND p.proposition_id=$3 \
           UNION ALL \
           SELECT 1 FROM niccolo.epistemic_hypotheses h \
            WHERE h.workspace_id=$1 AND h.situation_id=$2 AND h.hypothesis_id=$3 \
           UNION ALL \
           SELECT 1 FROM niccolo.epistemic_assumptions a \
            WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.assumption_id=$3 \
           UNION ALL \
           SELECT 1 FROM niccolo.epistemic_information_gaps g \
            WHERE g.workspace_id=$1 AND g.situation_id=$2 AND g.gap_id=$3 \
           UNION ALL \
           SELECT 1 FROM niccolo.epistemic_findings f \
            WHERE f.workspace_id=$1 AND f.situation_id=$2 AND f.finding_id=$3 \
           UNION ALL \
           SELECT 1 FROM niccolo.epistemic_arguments ar \
            WHERE ar.workspace_id=$1 AND ar.situation_id=$2 AND ar.argument_id=$3 \
         )",
    )
    .bind(workspace)
    .bind(situation)
    .bind(object_id)
    .fetch_one(&mut **tx)
    .await?)
}

async fn impacts_for_event(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    event_id: Uuid,
) -> Result<Vec<ReassessmentImpact>, ReassessmentError> {
    let rows: Vec<(Uuid, Uuid, i16, String)> = sqlx::query_as(
        "SELECT argument_id,conclusion_object_id,minimum_depth,impact_scope \
           FROM niccolo.epistemic_reassessment_impacts \
          WHERE workspace_id=$1 AND situation_id=$2 AND event_id=$3 \
          ORDER BY minimum_depth,argument_id",
    )
    .bind(workspace)
    .bind(situation)
    .bind(event_id)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(
            |(argument_id, conclusion_object_id, minimum_depth, impact_scope)| {
                Ok(ReassessmentImpact {
                    argument_id,
                    conclusion_object_id,
                    minimum_depth: u16::try_from(minimum_depth)
                        .map_err(|_| ReassessmentError::Invalid)?,
                    impact_scope,
                })
            },
        )
        .collect()
}

#[expect(
    clippy::too_many_arguments,
    reason = "reassessment identity, target, replacement, semantics and authorship are independent inputs"
)]
pub async fn record_object_change(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    event_id: Uuid,
    trigger_object_id: Uuid,
    replacement_object_id: Option<Uuid>,
    change_kind: ReassessmentChangeKind,
    rationale: &str,
) -> Result<ReassessmentEventView, ReassessmentError> {
    if !valid_text(rationale, 4096)
        || change_kind.requires_replacement() != replacement_object_id.is_some()
        || replacement_object_id == Some(trigger_object_id)
    {
        return Err(ReassessmentError::Invalid);
    }
    let rationale = rationale.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    if !object_visible_in_situation(&mut tx, workspace, situation, trigger_object_id).await? {
        return Err(ReassessmentError::Invisible);
    }
    if let Some(replacement) = replacement_object_id
        && !object_visible_in_situation(&mut tx, workspace, situation, replacement).await?
    {
        return Err(ReassessmentError::Invisible);
    }

    let inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_reassessment_events \
         (workspace_id,situation_id,event_id,trigger_object_id,replacement_object_id,change_kind,rationale,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(event_id)
    .bind(trigger_object_id)
    .bind(replacement_object_id)
    .bind(change_kind.db_value())
    .bind(rationale)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;

    let stored: Option<(Uuid, Option<Uuid>, String, String, Uuid)> = sqlx::query_as(
        "SELECT trigger_object_id,replacement_object_id,change_kind,rationale,recorded_by_principal_id \
           FROM niccolo.epistemic_reassessment_events \
          WHERE workspace_id=$1 AND situation_id=$2 AND event_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(event_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored
        != Some((
            trigger_object_id,
            replacement_object_id,
            change_kind.db_value().to_owned(),
            rationale.to_owned(),
            actor,
        ))
    {
        return Err(ReassessmentError::Conflict);
    }

    if inserted {
        sqlx::query(
            "WITH RECURSIVE affected(argument_id,conclusion_object_id,depth,path) AS ( \
               SELECT DISTINCT a.argument_id,a.conclusion_object_id,1::integer,ARRAY[a.argument_id]::uuid[] \
                 FROM niccolo.epistemic_grounding_members m \
                 JOIN niccolo.epistemic_arguments a \
                   ON a.workspace_id=m.workspace_id AND a.argument_id=m.argument_id \
                WHERE m.workspace_id=$1 AND a.situation_id=$2 AND m.ground_object_id=$4 \
               UNION ALL \
               SELECT DISTINCT a2.argument_id,a2.conclusion_object_id,(x.depth+1)::integer,x.path || a2.argument_id \
                 FROM affected x \
                 JOIN niccolo.epistemic_grounding_members m2 \
                   ON m2.workspace_id=$1 AND m2.ground_object_id=x.conclusion_object_id \
                 JOIN niccolo.epistemic_arguments a2 \
                   ON a2.workspace_id=m2.workspace_id AND a2.argument_id=m2.argument_id AND a2.situation_id=$2 \
                WHERE x.depth < 64 AND NOT (a2.argument_id = ANY(x.path)) \
             ), minimal AS ( \
               SELECT argument_id,conclusion_object_id,min(depth)::smallint AS minimum_depth \
                 FROM affected GROUP BY argument_id,conclusion_object_id \
             ) \
             INSERT INTO niccolo.epistemic_reassessment_impacts \
               (workspace_id,situation_id,event_id,argument_id,conclusion_object_id,minimum_depth,impact_scope) \
             SELECT $1,$2,$3,argument_id,conclusion_object_id,minimum_depth, \
                    CASE WHEN minimum_depth=1 THEN 'DIRECT_GROUND' ELSE 'DOWNSTREAM_CONCLUSION' END \
               FROM minimal ON CONFLICT DO NOTHING",
        )
        .bind(workspace)
        .bind(situation)
        .bind(event_id)
        .bind(trigger_object_id)
        .execute(&mut *tx)
        .await?;
    }

    let impacts = impacts_for_event(&mut tx, workspace, situation, event_id).await?;
    tx.commit().await?;
    Ok(ReassessmentEventView {
        event_id,
        situation_id: situation,
        trigger_object_id,
        replacement_object_id,
        change_kind,
        rationale: rationale.to_owned(),
        recorded_by: actor,
        impacts,
        automatic_conclusion_status_change: false,
        replayed: !inserted,
    })
}

pub async fn list(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    limit: i64,
) -> Result<Vec<ReassessmentEventView>, ReassessmentError> {
    if !(1..=100).contains(&limit) {
        return Err(ReassessmentError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    let rows: Vec<(Uuid, Uuid, Option<Uuid>, String, String, Uuid)> = sqlx::query_as(
        "SELECT event_id,trigger_object_id,replacement_object_id,change_kind,rationale,recorded_by_principal_id \
           FROM niccolo.epistemic_reassessment_events \
          WHERE workspace_id=$1 AND situation_id=$2 \
          ORDER BY created_at DESC,event_id DESC LIMIT $3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (event_id, trigger_object_id, replacement_object_id, change_kind, rationale, recorded_by) in
        rows
    {
        out.push(ReassessmentEventView {
            event_id,
            situation_id: situation,
            trigger_object_id,
            replacement_object_id,
            change_kind: ReassessmentChangeKind::from_db(&change_kind)?,
            rationale,
            recorded_by,
            impacts: impacts_for_event(&mut tx, workspace, situation, event_id).await?,
            automatic_conclusion_status_change: false,
            replayed: false,
        });
    }
    tx.commit().await?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_kind_replacement_contract_is_explicit() {
        assert!(ReassessmentChangeKind::Revised.requires_replacement());
        assert!(ReassessmentChangeKind::Superseded.requires_replacement());
        assert!(!ReassessmentChangeKind::Retracted.requires_replacement());
        assert!(!ReassessmentChangeKind::ManualReview.requires_replacement());
    }
}
