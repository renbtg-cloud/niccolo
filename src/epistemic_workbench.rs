//! Expert-authored epistemic workbench for the structured MVP.
//!
//! MVP users explicitly create epistemic abstractions and relationships. This module does
//! not parse investigator free text into abstractions; that compiler is a post-MVP layer.
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum WorkbenchError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid expert-workbench request")]
    Invalid,
    #[error("epistemic object is not visible in this Situation")]
    Invisible,
    #[error("epistemic authoring replay conflict")]
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExpertNodeKind {
    Proposition,
    Hypothesis,
    Assumption,
    InformationGap,
    Finding,
}
impl ExpertNodeKind {
    fn db_value(self) -> &'static str {
        match self {
            Self::Proposition => "PROPOSITION",
            Self::Hypothesis => "HYPOTHESIS",
            Self::Assumption => "ASSUMPTION",
            Self::InformationGap => "INFORMATION_GAP",
            Self::Finding => "FINDING",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpertNode {
    pub object_id: Uuid,
    pub kind: ExpertNodeKind,
    pub situation_id: Uuid,
    pub text: String,
    pub rationale: String,
    pub created_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelationFamily {
    Semantic,
    Genealogy,
    Contextual,
    Causal,
}
impl RelationFamily {
    fn db_value(self) -> &'static str {
        match self {
            Self::Semantic => "SEMANTIC",
            Self::Genealogy => "GENEALOGY",
            Self::Contextual => "CONTEXTUAL",
            Self::Causal => "CAUSAL",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelationKind {
    EquivalentTo,
    MoreSpecificThan,
    Entails,
    Contradicts,
    PartialOverlap,
    Refines,
    Supersedes,
    Retracts,
    Consolidates,
    Separates,
    Reinterprets,
    RelevantTo,
    ApplicableUnder,
    Causes,
    ContributesTo,
    Enables,
    Prevents,
    Intensifies,
    Attenuates,
}
impl RelationKind {
    fn db_value(self) -> &'static str {
        match self {
            Self::EquivalentTo => "EQUIVALENT_TO",
            Self::MoreSpecificThan => "MORE_SPECIFIC_THAN",
            Self::Entails => "ENTAILS",
            Self::Contradicts => "CONTRADICTS",
            Self::PartialOverlap => "PARTIAL_OVERLAP",
            Self::Refines => "REFINES",
            Self::Supersedes => "SUPERSEDES",
            Self::Retracts => "RETRACTS",
            Self::Consolidates => "CONSOLIDATES",
            Self::Separates => "SEPARATES",
            Self::Reinterprets => "REINTERPRETS",
            Self::RelevantTo => "RELEVANT_TO",
            Self::ApplicableUnder => "APPLICABLE_UNDER",
            Self::Causes => "CAUSES",
            Self::ContributesTo => "CONTRIBUTES_TO",
            Self::Enables => "ENABLES",
            Self::Prevents => "PREVENTS",
            Self::Intensifies => "INTENSIFIES",
            Self::Attenuates => "ATTENUATES",
        }
    }
    fn family(self) -> RelationFamily {
        match self {
            Self::EquivalentTo
            | Self::MoreSpecificThan
            | Self::Entails
            | Self::Contradicts
            | Self::PartialOverlap => RelationFamily::Semantic,
            Self::Refines
            | Self::Supersedes
            | Self::Retracts
            | Self::Consolidates
            | Self::Separates
            | Self::Reinterprets => RelationFamily::Genealogy,
            Self::RelevantTo | Self::ApplicableUnder => RelationFamily::Contextual,
            Self::Causes
            | Self::ContributesTo
            | Self::Enables
            | Self::Prevents
            | Self::Intensifies
            | Self::Attenuates => RelationFamily::Causal,
        }
    }
    fn symmetric(self) -> bool {
        matches!(
            self,
            Self::EquivalentTo | Self::Contradicts | Self::PartialOverlap
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchEvidence {
    pub evidence_id: Uuid,
    pub document_id: Uuid,
    pub ordinal: i32,
    pub speaker_label: String,
    pub content: String,
    pub source_class: String,
    pub source_locator: String,
    pub document_locator: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpertRelation {
    pub relation_id: Uuid,
    pub situation_id: Uuid,
    pub family: RelationFamily,
    pub kind: RelationKind,
    pub left_object_id: Uuid,
    pub right_object_id: Uuid,
    pub rationale: String,
    pub recorded_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ArgumentMode {
    Deductive,
    Inductive,
    Abductive,
    Causal,
    Analogical,
    Policy,
    Other,
}
impl ArgumentMode {
    fn db_value(self) -> &'static str {
        match self {
            Self::Deductive => "DEDUCTIVE",
            Self::Inductive => "INDUCTIVE",
            Self::Abductive => "ABDUCTIVE",
            Self::Causal => "CAUSAL",
            Self::Analogical => "ANALOGICAL",
            Self::Policy => "POLICY",
            Self::Other => "OTHER",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpertArgument {
    pub argument_id: Uuid,
    pub situation_id: Uuid,
    pub conclusion_object_id: Uuid,
    pub mode: ArgumentMode,
    pub rationale: String,
    pub created_by: Uuid,
    pub replayed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroundingEffect {
    Supports,
    Weakens,
    Challenges,
    Conditions,
}
impl GroundingEffect {
    fn db_value(self) -> &'static str {
        match self {
            Self::Supports => "SUPPORTS",
            Self::Weakens => "WEAKENS",
            Self::Challenges => "CHALLENGES",
            Self::Conditions => "CONDITIONS",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroundingAdequacy {
    Unspecified,
    Partial,
    Sufficient,
    NecessaryCondition,
}
impl GroundingAdequacy {
    fn db_value(self) -> &'static str {
        match self {
            Self::Unspecified => "UNSPECIFIED",
            Self::Partial => "PARTIAL",
            Self::Sufficient => "SUFFICIENT",
            Self::NecessaryCondition => "NECESSARY_CONDITION",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GroundingMemberRole {
    Premise,
    Assumption,
    Evidence,
    Context,
}
impl GroundingMemberRole {
    fn db_value(self) -> &'static str {
        match self {
            Self::Premise => "PREMISE",
            Self::Assumption => "ASSUMPTION",
            Self::Evidence => "EVIDENCE",
            Self::Context => "CONTEXT",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewGroundingMember {
    pub member_id: Uuid,
    pub role: GroundingMemberRole,
    pub ground_object_id: Option<Uuid>,
    pub evidence_id: Option<Uuid>,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundingMemberView {
    pub member_id: Uuid,
    pub role: GroundingMemberRole,
    pub ground_object_id: Option<Uuid>,
    pub evidence_id: Option<Uuid>,
    pub rationale: String,
    pub recorded_by: Uuid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroundingSetView {
    pub argument_id: Uuid,
    pub grounding_set_id: Uuid,
    pub effect: GroundingEffect,
    pub adequacy: GroundingAdequacy,
    pub rationale: String,
    pub recorded_by: Uuid,
    pub members: Vec<GroundingMemberView>,
    pub replayed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkbenchSnapshot {
    pub situation_id: Uuid,
    pub nodes: Vec<ExpertNode>,
    pub evidence: Vec<WorkbenchEvidence>,
    pub evidence_truncated: bool,
    pub relations: Vec<ExpertRelation>,
    pub arguments: Vec<ExpertArgument>,
    pub grounding_sets: Vec<GroundingSetView>,
    pub investigator_free_text_compiler_enabled: bool,
}

type WorkbenchEvidenceRow = (Uuid, Uuid, i32, String, String, String, String, String);
type StoredGroundingMemberRow = (String, Option<Uuid>, Option<Uuid>, String, Uuid);
type GroundingMemberSnapshotRow = (
    Uuid,
    Uuid,
    Uuid,
    String,
    Option<Uuid>,
    Option<Uuid>,
    String,
    Uuid,
);

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    read_only: bool,
) -> Result<Transaction<'_, Postgres>, WorkbenchError> {
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
        return Err(WorkbenchError::Privileged);
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

async fn situation_visible(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<bool, WorkbenchError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.situations WHERE workspace_id=$1 AND situation_id=$2)",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_one(&mut **tx)
    .await?)
}

async fn ensure_object(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    object_id: Uuid,
    object_kind: &str,
    actor: Uuid,
) -> Result<bool, WorkbenchError> {
    let inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_objects(workspace_id,object_id,object_kind,created_by_principal_id) \
         VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(object_id)
    .bind(object_kind)
    .bind(actor)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        == 1;
    let stored: Option<(String, Uuid)> = sqlx::query_as(
        "SELECT object_kind,created_by_principal_id FROM niccolo.epistemic_objects \
         WHERE workspace_id=$1 AND object_id=$2",
    )
    .bind(workspace)
    .bind(object_id)
    .fetch_optional(&mut **tx)
    .await?;
    match stored {
        Some((kind, by)) if kind == object_kind && by == actor => Ok(inserted),
        Some(_) => Err(WorkbenchError::Conflict),
        None => Err(WorkbenchError::Invisible),
    }
}

async fn object_available_in_situation(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    object_id: Uuid,
) -> Result<Option<String>, WorkbenchError> {
    let kind: Option<String> = sqlx::query_scalar(
        "SELECT object_kind FROM niccolo.epistemic_objects WHERE workspace_id=$1 AND object_id=$2",
    )
    .bind(workspace)
    .bind(object_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(kind) = kind else {
        return Ok(None);
    };
    let available: bool = match kind.as_str() {
        "PROPOSITION" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_situation_propositions WHERE workspace_id=$1 AND situation_id=$2 AND proposition_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_one(&mut **tx)
        .await?,
        "HYPOTHESIS" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_hypotheses WHERE workspace_id=$1 AND situation_id=$2 AND hypothesis_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_one(&mut **tx)
        .await?,
        "ASSUMPTION" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_assumptions WHERE workspace_id=$1 AND situation_id=$2 AND assumption_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_one(&mut **tx)
        .await?,
        "INFORMATION_GAP" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_information_gaps WHERE workspace_id=$1 AND situation_id=$2 AND gap_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_one(&mut **tx)
        .await?,
        "FINDING" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_findings WHERE workspace_id=$1 AND situation_id=$2 AND finding_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_one(&mut **tx)
        .await?,
        "ARGUMENT" => sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_arguments WHERE workspace_id=$1 AND situation_id=$2 AND argument_id=$3)",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_one(&mut **tx)
        .await?,
        _ => false,
    };
    Ok(available.then_some(kind))
}

#[expect(
    clippy::too_many_arguments,
    reason = "node identity, scope, semantics and authorship are independent inputs"
)]
pub async fn create_node(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    object_id: Uuid,
    kind: ExpertNodeKind,
    text: &str,
    rationale: &str,
) -> Result<ExpertNode, WorkbenchError> {
    if !valid_text(text, 8192) || !valid_text(rationale, 4096) {
        return Err(WorkbenchError::Invalid);
    }
    let text = text.trim();
    let rationale = rationale.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    if !situation_visible(&mut tx, workspace, situation).await? {
        return Err(WorkbenchError::Invisible);
    }
    let object_inserted = ensure_object(&mut tx, workspace, object_id, kind.db_value(), actor).await?;
    let mut request_replayed = !object_inserted;
    if kind == ExpertNodeKind::Proposition {
        sqlx::query(
            "INSERT INTO niccolo.epistemic_propositions(workspace_id,proposition_id,statement_text,language) \
             VALUES($1,$2,$3,'en') ON CONFLICT DO NOTHING",
        )
        .bind(workspace)
        .bind(object_id)
        .bind(text)
        .execute(&mut *tx)
        .await?;
        let stored_text: Option<String> = sqlx::query_scalar(
            "SELECT statement_text FROM niccolo.epistemic_propositions WHERE workspace_id=$1 AND proposition_id=$2",
        )
        .bind(workspace)
        .bind(object_id)
        .fetch_optional(&mut *tx)
        .await?;
        if stored_text.as_deref() != Some(text) {
            return Err(WorkbenchError::Conflict);
        }
        let membership_inserted = sqlx::query(
            "INSERT INTO niccolo.epistemic_situation_propositions \
             (workspace_id,situation_id,proposition_id,rationale,recorded_by_principal_id) \
             VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .bind(rationale)
        .bind(actor)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        request_replayed = !membership_inserted;
        let membership: Option<(String, Uuid)> = sqlx::query_as(
            "SELECT rationale,recorded_by_principal_id FROM niccolo.epistemic_situation_propositions \
             WHERE workspace_id=$1 AND situation_id=$2 AND proposition_id=$3",
        )
        .bind(workspace)
        .bind(situation)
        .bind(object_id)
        .fetch_optional(&mut *tx)
        .await?;
        if membership != Some((rationale.to_owned(), actor)) {
            return Err(WorkbenchError::Conflict);
        }
    } else {
        let sql = match kind {
            ExpertNodeKind::Proposition => unreachable!(),
            ExpertNodeKind::Hypothesis => {
                "INSERT INTO niccolo.epistemic_hypotheses(workspace_id,hypothesis_id,situation_id,statement_text,rationale) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING"
            }
            ExpertNodeKind::Assumption => {
                "INSERT INTO niccolo.epistemic_assumptions(workspace_id,assumption_id,situation_id,statement_text,rationale) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING"
            }
            ExpertNodeKind::InformationGap => {
                "INSERT INTO niccolo.epistemic_information_gaps(workspace_id,gap_id,situation_id,description,rationale) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING"
            }
            ExpertNodeKind::Finding => {
                "INSERT INTO niccolo.epistemic_findings(workspace_id,finding_id,situation_id,statement_text,rationale) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING"
            }
        };
        sqlx::query(sql)
            .bind(workspace)
            .bind(object_id)
            .bind(situation)
            .bind(text)
            .bind(rationale)
            .execute(&mut *tx)
            .await?;
        let query = match kind {
            ExpertNodeKind::Proposition => unreachable!(),
            ExpertNodeKind::Hypothesis => {
                "SELECT situation_id,statement_text,rationale FROM niccolo.epistemic_hypotheses WHERE workspace_id=$1 AND hypothesis_id=$2"
            }
            ExpertNodeKind::Assumption => {
                "SELECT situation_id,statement_text,rationale FROM niccolo.epistemic_assumptions WHERE workspace_id=$1 AND assumption_id=$2"
            }
            ExpertNodeKind::InformationGap => {
                "SELECT situation_id,description,rationale FROM niccolo.epistemic_information_gaps WHERE workspace_id=$1 AND gap_id=$2"
            }
            ExpertNodeKind::Finding => {
                "SELECT situation_id,statement_text,rationale FROM niccolo.epistemic_findings WHERE workspace_id=$1 AND finding_id=$2"
            }
        };
        let stored: Option<(Uuid, String, String)> = sqlx::query_as(query)
            .bind(workspace)
            .bind(object_id)
            .fetch_optional(&mut *tx)
            .await?;
        if stored != Some((situation, text.to_owned(), rationale.to_owned())) {
            return Err(WorkbenchError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(ExpertNode {
        object_id,
        kind,
        situation_id: situation,
        text: text.to_owned(),
        rationale: rationale.to_owned(),
        created_by: actor,
        replayed: request_replayed,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "relation identity, endpoints, semantics and authorship are independent inputs"
)]
pub async fn create_relation(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    relation_id: Uuid,
    family: RelationFamily,
    kind: RelationKind,
    mut left_object_id: Uuid,
    mut right_object_id: Uuid,
    rationale: &str,
) -> Result<ExpertRelation, WorkbenchError> {
    if kind.family() != family
        || left_object_id == right_object_id
        || !valid_text(rationale, 4096)
    {
        return Err(WorkbenchError::Invalid);
    }
    if kind.symmetric() && right_object_id < left_object_id {
        std::mem::swap(&mut left_object_id, &mut right_object_id);
    }
    let rationale = rationale.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    if object_available_in_situation(&mut tx, workspace, situation, left_object_id)
        .await?
        .is_none()
        || object_available_in_situation(&mut tx, workspace, situation, right_object_id)
            .await?
            .is_none()
    {
        return Err(WorkbenchError::Invisible);
    }
    let inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_relations \
         (workspace_id,situation_id,relation_id,relation_family,relation_kind,left_object_id,right_object_id,rationale,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(relation_id)
    .bind(family.db_value())
    .bind(kind.db_value())
    .bind(left_object_id)
    .bind(right_object_id)
    .bind(rationale)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let stored: Option<(Uuid, String, String, Uuid, Uuid, String, Uuid)> = sqlx::query_as(
        "SELECT situation_id,relation_family,relation_kind,left_object_id,right_object_id,rationale,recorded_by_principal_id \
         FROM niccolo.epistemic_relations WHERE workspace_id=$1 AND relation_id=$2",
    )
    .bind(workspace)
    .bind(relation_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored
        != Some((
            situation,
            family.db_value().to_owned(),
            kind.db_value().to_owned(),
            left_object_id,
            right_object_id,
            rationale.to_owned(),
            actor,
        ))
    {
        return Err(WorkbenchError::Conflict);
    }
    tx.commit().await?;
    Ok(ExpertRelation {
        relation_id,
        situation_id: situation,
        family,
        kind,
        left_object_id,
        right_object_id,
        rationale: rationale.to_owned(),
        recorded_by: actor,
        replayed: !inserted,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "argument identity, conclusion, method and authorship are independent inputs"
)]
pub async fn create_argument(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    argument_id: Uuid,
    conclusion_object_id: Uuid,
    mode: ArgumentMode,
    rationale: &str,
) -> Result<ExpertArgument, WorkbenchError> {
    if argument_id == conclusion_object_id || !valid_text(rationale, 4096) {
        return Err(WorkbenchError::Invalid);
    }
    let rationale = rationale.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    let conclusion_kind = object_available_in_situation(
        &mut tx,
        workspace,
        situation,
        conclusion_object_id,
    )
    .await?;
    if conclusion_kind.is_none() || conclusion_kind.as_deref() == Some("ARGUMENT") {
        return Err(WorkbenchError::Invisible);
    }
    let object_inserted = ensure_object(&mut tx, workspace, argument_id, "ARGUMENT", actor).await?;
    sqlx::query(
        "INSERT INTO niccolo.epistemic_arguments \
         (workspace_id,argument_id,situation_id,conclusion_object_id,argument_mode,rationale) \
         VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(argument_id)
    .bind(situation)
    .bind(conclusion_object_id)
    .bind(mode.db_value())
    .bind(rationale)
    .execute(&mut *tx)
    .await?;
    let stored: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT situation_id,conclusion_object_id,argument_mode,rationale \
         FROM niccolo.epistemic_arguments WHERE workspace_id=$1 AND argument_id=$2",
    )
    .bind(workspace)
    .bind(argument_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored
        != Some((
            situation,
            conclusion_object_id,
            mode.db_value().to_owned(),
            rationale.to_owned(),
        ))
    {
        return Err(WorkbenchError::Conflict);
    }
    tx.commit().await?;
    Ok(ExpertArgument {
        argument_id,
        situation_id: situation,
        conclusion_object_id,
        mode,
        rationale: rationale.to_owned(),
        created_by: actor,
        replayed: !object_inserted,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "grounding identity, semantics and members are independent authored inputs"
)]
pub async fn create_grounding_set(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    actor: Uuid,
    argument_id: Uuid,
    grounding_set_id: Uuid,
    effect: GroundingEffect,
    adequacy: GroundingAdequacy,
    rationale: &str,
    members: &[NewGroundingMember],
) -> Result<GroundingSetView, WorkbenchError> {
    if !valid_text(rationale, 4096)
        || members.is_empty()
        || members.len() > 64
        || members.iter().map(|m| m.member_id).collect::<BTreeSet<_>>().len() != members.len()
    {
        return Err(WorkbenchError::Invalid);
    }
    for member in members {
        let one_target = member.ground_object_id.is_some() ^ member.evidence_id.is_some();
        if !one_target
            || !valid_text(&member.rationale, 2048)
            || (member.role == GroundingMemberRole::Evidence) != member.evidence_id.is_some()
        {
            return Err(WorkbenchError::Invalid);
        }
    }
    let rationale = rationale.trim();
    let mut tx = scoped(pool, workspace, false).await?;
    let argument_visible: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM niccolo.epistemic_arguments \
         WHERE workspace_id=$1 AND situation_id=$2 AND argument_id=$3)",
    )
    .bind(workspace)
    .bind(situation)
    .bind(argument_id)
    .fetch_one(&mut *tx)
    .await?;
    if !argument_visible {
        return Err(WorkbenchError::Invisible);
    }
    for member in members {
        if let Some(object_id) = member.ground_object_id
            && object_available_in_situation(&mut tx, workspace, situation, object_id)
                .await?
                .is_none()
        {
            return Err(WorkbenchError::Invisible);
        }
        if let Some(evidence_id) = member.evidence_id {
            let visible: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM niccolo.source_fragments f \
                 JOIN niccolo.situation_sources s ON s.workspace_id=f.workspace_id AND s.document_id=f.document_id \
                 WHERE f.workspace_id=$1 AND s.situation_id=$2 AND f.evidence_id=$3)",
            )
            .bind(workspace)
            .bind(situation)
            .bind(evidence_id)
            .fetch_one(&mut *tx)
            .await?;
            if !visible {
                return Err(WorkbenchError::Invisible);
            }
        }
    }
    let inserted = sqlx::query(
        "INSERT INTO niccolo.epistemic_grounding_sets \
         (workspace_id,argument_id,grounding_set_id,effect,adequacy,rationale,recorded_by_principal_id) \
         VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
    )
    .bind(workspace)
    .bind(argument_id)
    .bind(grounding_set_id)
    .bind(effect.db_value())
    .bind(adequacy.db_value())
    .bind(rationale)
    .bind(actor)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let stored_set: Option<(String, String, String, Uuid)> = sqlx::query_as(
        "SELECT effect,adequacy,rationale,recorded_by_principal_id \
         FROM niccolo.epistemic_grounding_sets \
         WHERE workspace_id=$1 AND argument_id=$2 AND grounding_set_id=$3",
    )
    .bind(workspace)
    .bind(argument_id)
    .bind(grounding_set_id)
    .fetch_optional(&mut *tx)
    .await?;
    if stored_set
        != Some((
            effect.db_value().to_owned(),
            adequacy.db_value().to_owned(),
            rationale.to_owned(),
            actor,
        ))
    {
        return Err(WorkbenchError::Conflict);
    }
    for member in members {
        sqlx::query(
            "INSERT INTO niccolo.epistemic_grounding_members \
             (workspace_id,argument_id,grounding_set_id,member_id,member_role,ground_object_id,evidence_id,rationale,recorded_by_principal_id) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
        )
        .bind(workspace)
        .bind(argument_id)
        .bind(grounding_set_id)
        .bind(member.member_id)
        .bind(member.role.db_value())
        .bind(member.ground_object_id)
        .bind(member.evidence_id)
        .bind(member.rationale.trim())
        .bind(actor)
        .execute(&mut *tx)
        .await?;
        let stored: Option<StoredGroundingMemberRow> = sqlx::query_as(
            "SELECT member_role,ground_object_id,evidence_id,rationale,recorded_by_principal_id \
             FROM niccolo.epistemic_grounding_members \
             WHERE workspace_id=$1 AND argument_id=$2 AND grounding_set_id=$3 AND member_id=$4",
        )
        .bind(workspace)
        .bind(argument_id)
        .bind(grounding_set_id)
        .bind(member.member_id)
        .fetch_optional(&mut *tx)
        .await?;
        if stored
            != Some((
                member.role.db_value().to_owned(),
                member.ground_object_id,
                member.evidence_id,
                member.rationale.trim().to_owned(),
                actor,
            ))
        {
            return Err(WorkbenchError::Conflict);
        }
    }
    let stored_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM niccolo.epistemic_grounding_members \
         WHERE workspace_id=$1 AND argument_id=$2 AND grounding_set_id=$3",
    )
    .bind(workspace)
    .bind(argument_id)
    .bind(grounding_set_id)
    .fetch_one(&mut *tx)
    .await?;
    if usize::try_from(stored_count).ok() != Some(members.len()) {
        return Err(WorkbenchError::Conflict);
    }
    tx.commit().await?;
    Ok(GroundingSetView {
        argument_id,
        grounding_set_id,
        effect,
        adequacy,
        rationale: rationale.to_owned(),
        recorded_by: actor,
        members: members
            .iter()
            .map(|m| GroundingMemberView {
                member_id: m.member_id,
                role: m.role,
                ground_object_id: m.ground_object_id,
                evidence_id: m.evidence_id,
                rationale: m.rationale.trim().to_owned(),
                recorded_by: actor,
            })
            .collect(),
        replayed: !inserted,
    })
}

fn node_kind_from_db(value: &str) -> Result<ExpertNodeKind, WorkbenchError> {
    match value {
        "PROPOSITION" => Ok(ExpertNodeKind::Proposition),
        "HYPOTHESIS" => Ok(ExpertNodeKind::Hypothesis),
        "ASSUMPTION" => Ok(ExpertNodeKind::Assumption),
        "INFORMATION_GAP" => Ok(ExpertNodeKind::InformationGap),
        "FINDING" => Ok(ExpertNodeKind::Finding),
        _ => Err(WorkbenchError::Invalid),
    }
}
fn relation_family_from_db(value: &str) -> Result<RelationFamily, WorkbenchError> {
    match value {
        "SEMANTIC" => Ok(RelationFamily::Semantic),
        "GENEALOGY" => Ok(RelationFamily::Genealogy),
        "CONTEXTUAL" => Ok(RelationFamily::Contextual),
        "CAUSAL" => Ok(RelationFamily::Causal),
        _ => Err(WorkbenchError::Invalid),
    }
}
fn relation_kind_from_db(value: &str) -> Result<RelationKind, WorkbenchError> {
    match value {
        "EQUIVALENT_TO" => Ok(RelationKind::EquivalentTo),
        "MORE_SPECIFIC_THAN" => Ok(RelationKind::MoreSpecificThan),
        "ENTAILS" => Ok(RelationKind::Entails),
        "CONTRADICTS" => Ok(RelationKind::Contradicts),
        "PARTIAL_OVERLAP" => Ok(RelationKind::PartialOverlap),
        "REFINES" => Ok(RelationKind::Refines),
        "SUPERSEDES" => Ok(RelationKind::Supersedes),
        "RETRACTS" => Ok(RelationKind::Retracts),
        "CONSOLIDATES" => Ok(RelationKind::Consolidates),
        "SEPARATES" => Ok(RelationKind::Separates),
        "REINTERPRETS" => Ok(RelationKind::Reinterprets),
        "RELEVANT_TO" => Ok(RelationKind::RelevantTo),
        "APPLICABLE_UNDER" => Ok(RelationKind::ApplicableUnder),
        "CAUSES" => Ok(RelationKind::Causes),
        "CONTRIBUTES_TO" => Ok(RelationKind::ContributesTo),
        "ENABLES" => Ok(RelationKind::Enables),
        "PREVENTS" => Ok(RelationKind::Prevents),
        "INTENSIFIES" => Ok(RelationKind::Intensifies),
        "ATTENUATES" => Ok(RelationKind::Attenuates),
        _ => Err(WorkbenchError::Invalid),
    }
}
fn argument_mode_from_db(value: &str) -> Result<ArgumentMode, WorkbenchError> {
    match value {
        "DEDUCTIVE" => Ok(ArgumentMode::Deductive),
        "INDUCTIVE" => Ok(ArgumentMode::Inductive),
        "ABDUCTIVE" => Ok(ArgumentMode::Abductive),
        "CAUSAL" => Ok(ArgumentMode::Causal),
        "ANALOGICAL" => Ok(ArgumentMode::Analogical),
        "POLICY" => Ok(ArgumentMode::Policy),
        "OTHER" => Ok(ArgumentMode::Other),
        _ => Err(WorkbenchError::Invalid),
    }
}
fn grounding_effect_from_db(value: &str) -> Result<GroundingEffect, WorkbenchError> {
    match value {
        "SUPPORTS" => Ok(GroundingEffect::Supports),
        "WEAKENS" => Ok(GroundingEffect::Weakens),
        "CHALLENGES" => Ok(GroundingEffect::Challenges),
        "CONDITIONS" => Ok(GroundingEffect::Conditions),
        _ => Err(WorkbenchError::Invalid),
    }
}
fn grounding_adequacy_from_db(value: &str) -> Result<GroundingAdequacy, WorkbenchError> {
    match value {
        "UNSPECIFIED" => Ok(GroundingAdequacy::Unspecified),
        "PARTIAL" => Ok(GroundingAdequacy::Partial),
        "SUFFICIENT" => Ok(GroundingAdequacy::Sufficient),
        "NECESSARY_CONDITION" => Ok(GroundingAdequacy::NecessaryCondition),
        _ => Err(WorkbenchError::Invalid),
    }
}
fn member_role_from_db(value: &str) -> Result<GroundingMemberRole, WorkbenchError> {
    match value {
        "PREMISE" => Ok(GroundingMemberRole::Premise),
        "ASSUMPTION" => Ok(GroundingMemberRole::Assumption),
        "EVIDENCE" => Ok(GroundingMemberRole::Evidence),
        "CONTEXT" => Ok(GroundingMemberRole::Context),
        _ => Err(WorkbenchError::Invalid),
    }
}

pub async fn snapshot(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
) -> Result<WorkbenchSnapshot, WorkbenchError> {
    let mut tx = scoped(pool, workspace, true).await?;
    if !situation_visible(&mut tx, workspace, situation).await? {
        return Err(WorkbenchError::Invisible);
    }
    let node_rows: Vec<(Uuid, String, String, String, Uuid)> = sqlx::query_as(
        "SELECT object_id,kind,text,rationale,created_by FROM ( \
           SELECT p.proposition_id AS object_id,'PROPOSITION'::text AS kind,p.statement_text AS text,sp.rationale,o.created_by_principal_id AS created_by,sp.created_at \
             FROM niccolo.epistemic_situation_propositions sp \
             JOIN niccolo.epistemic_propositions p ON p.workspace_id=sp.workspace_id AND p.proposition_id=sp.proposition_id \
             JOIN niccolo.epistemic_objects o ON o.workspace_id=p.workspace_id AND o.object_id=p.proposition_id \
            WHERE sp.workspace_id=$1 AND sp.situation_id=$2 \
           UNION ALL \
           SELECT h.hypothesis_id AS object_id,'HYPOTHESIS'::text AS kind,h.statement_text AS text,h.rationale,o.created_by_principal_id AS created_by,h.created_at \
             FROM niccolo.epistemic_hypotheses h JOIN niccolo.epistemic_objects o ON o.workspace_id=h.workspace_id AND o.object_id=h.hypothesis_id \
            WHERE h.workspace_id=$1 AND h.situation_id=$2 \
           UNION ALL \
           SELECT a.assumption_id,'ASSUMPTION',a.statement_text,a.rationale,o.created_by_principal_id,a.created_at \
             FROM niccolo.epistemic_assumptions a JOIN niccolo.epistemic_objects o ON o.workspace_id=a.workspace_id AND o.object_id=a.assumption_id \
            WHERE a.workspace_id=$1 AND a.situation_id=$2 \
           UNION ALL \
           SELECT g.gap_id,'INFORMATION_GAP',g.description,g.rationale,o.created_by_principal_id,g.created_at \
             FROM niccolo.epistemic_information_gaps g JOIN niccolo.epistemic_objects o ON o.workspace_id=g.workspace_id AND o.object_id=g.gap_id \
            WHERE g.workspace_id=$1 AND g.situation_id=$2 \
           UNION ALL \
           SELECT f.finding_id,'FINDING',f.statement_text,f.rationale,o.created_by_principal_id,f.created_at \
             FROM niccolo.epistemic_findings f JOIN niccolo.epistemic_objects o ON o.workspace_id=f.workspace_id AND o.object_id=f.finding_id \
            WHERE f.workspace_id=$1 AND f.situation_id=$2 \
         ) q ORDER BY created_at,object_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    let mut nodes = Vec::with_capacity(node_rows.len());
    for (object_id, kind, text, rationale, created_by) in node_rows {
        nodes.push(ExpertNode {
            object_id,
            kind: node_kind_from_db(&kind)?,
            situation_id: situation,
            text,
            rationale,
            created_by,
            replayed: false,
        });
    }
    let evidence_rows: Vec<WorkbenchEvidenceRow> = sqlx::query_as(
        "SELECT f.evidence_id,f.document_id,f.ordinal,f.speaker_label,e.content,e.source_class,e.source_locator,d.locator \
         FROM niccolo.situation_sources ss \
         JOIN niccolo.source_fragments f ON f.workspace_id=ss.workspace_id AND f.document_id=ss.document_id \
         JOIN niccolo.evidence e ON e.workspace_id=f.workspace_id AND e.evidence_id=f.evidence_id \
         JOIN niccolo.source_documents d ON d.workspace_id=f.workspace_id AND d.document_id=f.document_id \
         WHERE ss.workspace_id=$1 AND ss.situation_id=$2 AND e.admissible \
         ORDER BY d.created_at,f.document_id,f.ordinal LIMIT 257",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    let evidence_truncated = evidence_rows.len() > 256;
    let evidence = evidence_rows
        .into_iter()
        .take(256)
        .map(
            |(evidence_id, document_id, ordinal, speaker_label, content, source_class, source_locator, document_locator)| WorkbenchEvidence {
                evidence_id,
                document_id,
                ordinal,
                speaker_label,
                content,
                source_class,
                source_locator,
                document_locator,
            },
        )
        .collect();

    let relation_rows: Vec<(Uuid, String, String, Uuid, Uuid, String, Uuid)> = sqlx::query_as(
        "SELECT relation_id,relation_family,relation_kind,left_object_id,right_object_id,rationale,recorded_by_principal_id \
         FROM niccolo.epistemic_relations WHERE workspace_id=$1 AND situation_id=$2 \
         ORDER BY created_at,relation_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    let mut relations = Vec::with_capacity(relation_rows.len());
    for (relation_id, family, kind, left_object_id, right_object_id, rationale, recorded_by) in
        relation_rows
    {
        relations.push(ExpertRelation {
            relation_id,
            situation_id: situation,
            family: relation_family_from_db(&family)?,
            kind: relation_kind_from_db(&kind)?,
            left_object_id,
            right_object_id,
            rationale,
            recorded_by,
            replayed: false,
        });
    }
    let argument_rows: Vec<(Uuid, Uuid, String, String, Uuid)> = sqlx::query_as(
        "SELECT a.argument_id,a.conclusion_object_id,a.argument_mode,a.rationale,o.created_by_principal_id \
         FROM niccolo.epistemic_arguments a JOIN niccolo.epistemic_objects o \
           ON o.workspace_id=a.workspace_id AND o.object_id=a.argument_id \
         WHERE a.workspace_id=$1 AND a.situation_id=$2 ORDER BY a.created_at,a.argument_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    let mut arguments = Vec::with_capacity(argument_rows.len());
    for (argument_id, conclusion_object_id, mode, rationale, created_by) in argument_rows {
        arguments.push(ExpertArgument {
            argument_id,
            situation_id: situation,
            conclusion_object_id,
            mode: argument_mode_from_db(&mode)?,
            rationale,
            created_by,
            replayed: false,
        });
    }
    let set_rows: Vec<(Uuid, Uuid, String, String, String, Uuid)> = sqlx::query_as(
        "SELECT s.argument_id,s.grounding_set_id,s.effect,s.adequacy,s.rationale,s.recorded_by_principal_id \
         FROM niccolo.epistemic_grounding_sets s JOIN niccolo.epistemic_arguments a \
           ON a.workspace_id=s.workspace_id AND a.argument_id=s.argument_id \
         WHERE s.workspace_id=$1 AND a.situation_id=$2 ORDER BY s.created_at,s.grounding_set_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut *tx)
    .await?;
    let member_rows: Vec<GroundingMemberSnapshotRow> = sqlx::query_as(
            "SELECT m.argument_id,m.grounding_set_id,m.member_id,m.member_role,m.ground_object_id,m.evidence_id,m.rationale,m.recorded_by_principal_id \
             FROM niccolo.epistemic_grounding_members m JOIN niccolo.epistemic_arguments a \
               ON a.workspace_id=m.workspace_id AND a.argument_id=m.argument_id \
             WHERE m.workspace_id=$1 AND a.situation_id=$2 ORDER BY m.created_at,m.member_id",
        )
        .bind(workspace)
        .bind(situation)
        .fetch_all(&mut *tx)
        .await?;
    let mut grouped: BTreeMap<(Uuid, Uuid), Vec<GroundingMemberView>> = BTreeMap::new();
    for (argument_id, grounding_set_id, member_id, role, ground_object_id, evidence_id, rationale, recorded_by) in member_rows {
        grouped
            .entry((argument_id, grounding_set_id))
            .or_default()
            .push(GroundingMemberView {
                member_id,
                role: member_role_from_db(&role)?,
                ground_object_id,
                evidence_id,
                rationale,
                recorded_by,
            });
    }
    let mut grounding_sets = Vec::with_capacity(set_rows.len());
    for (argument_id, grounding_set_id, effect, adequacy, rationale, recorded_by) in set_rows {
        grounding_sets.push(GroundingSetView {
            argument_id,
            grounding_set_id,
            effect: grounding_effect_from_db(&effect)?,
            adequacy: grounding_adequacy_from_db(&adequacy)?,
            rationale,
            recorded_by,
            members: grouped
                .remove(&(argument_id, grounding_set_id))
                .unwrap_or_default(),
            replayed: false,
        });
    }
    tx.commit().await?;
    Ok(WorkbenchSnapshot {
        situation_id: situation,
        nodes,
        evidence,
        evidence_truncated,
        relations,
        arguments,
        grounding_sets,
        investigator_free_text_compiler_enabled: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_family_is_not_inferred_from_text() {
        assert_eq!(RelationKind::Contradicts.family(), RelationFamily::Semantic);
        assert_eq!(RelationKind::Causes.family(), RelationFamily::Causal);
        assert!(RelationKind::Contradicts.symmetric());
        assert!(!RelationKind::Causes.symmetric());
    }

    #[test]
    fn mvp_free_text_compiler_is_not_part_of_the_workbench_contract() {
        let value = serde_json::to_value(WorkbenchSnapshot {
            situation_id: Uuid::nil(),
            nodes: vec![],
            evidence: vec![],
            evidence_truncated: false,
            relations: vec![],
            arguments: vec![],
            grounding_sets: vec![],
            investigator_free_text_compiler_enabled: false,
        })
        .unwrap();
        assert_eq!(
            value["investigator_free_text_compiler_enabled"],
            serde_json::Value::Bool(false)
        );
    }
}
