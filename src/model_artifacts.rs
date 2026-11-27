//! v0.601-v0.650: validated, atomic synthesis artifact ingestion.
//! A model response is recorded as analysis output/claims; recording does not establish truth.
use crate::analysis_structure::AnalysisScope;
use crate::analysis_workbench_support::{self, SupportLink};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("invalid synthesis artifact")]
    Invalid,
    #[error("restricted MVP hypotheses and assumptions must be authored by a human")]
    HumanAuthorshipRequired,
    #[error("synthesis cited a Workbench route outside the frozen usable support")]
    WorkbenchSupportUnavailable,
    #[error("artifact replay conflicts with stored analysis")]
    Conflict,
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactClaim {
    pub claim_id: Uuid,
    pub ordinal: i32,
    pub kind: String,
    pub text: String,
    pub qualification: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSourceLink {
    pub claim_id: Uuid,
    pub evidence_id: Uuid,
    pub relation: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactContextLink {
    pub claim_id: Uuid,
    pub contribution_id: Uuid,
    pub relation: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactAssumption {
    pub assumption_id: Uuid,
    pub claim_id: Option<Uuid>,
    pub statement: String,
    pub basis_note: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactAlternative {
    pub alternative_id: Uuid,
    pub claim_id: Uuid,
    pub text: String,
    pub rationale: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactGap {
    pub gap_id: Uuid,
    pub claim_id: Option<Uuid>,
    pub description: String,
    pub requested_source_kind: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactClarification {
    pub question_id: Uuid,
    pub text: String,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SynthesisArtifact {
    pub answer_text: String,
    pub coverage_note: String,
    #[serde(default)]
    pub claims: Vec<ArtifactClaim>,
    #[serde(default)]
    pub source_links: Vec<ArtifactSourceLink>,
    #[serde(default)]
    pub context_links: Vec<ArtifactContextLink>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workbench_support_links: Vec<SupportLink>,
    #[serde(default)]
    pub assumptions: Vec<ArtifactAssumption>,
    #[serde(default)]
    pub alternatives: Vec<ArtifactAlternative>,
    #[serde(default)]
    pub evidence_gaps: Vec<ArtifactGap>,
    #[serde(default)]
    pub clarifications: Vec<ArtifactClarification>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedArtifact {
    pub claim_count: usize,
    pub source_link_count: usize,
    pub context_link_count: usize,
    pub assumption_count: usize,
    pub alternative_count: usize,
    pub evidence_gap_count: usize,
    pub clarification_count: usize,
    pub replayed: bool,
    pub recorded_output_is_truth: bool,
}
fn valid_text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.contains('\0')
}
fn valid_kind(s: &str) -> bool {
    matches!(
        s,
        "OBSERVATION" | "INFERENCE" | "POSSIBILITY" | "RECOMMENDATION" | "UNCERTAINTY"
    )
}
fn validate(a: &SynthesisArtifact) -> Result<(), ArtifactError> {
    if !valid_text(&a.answer_text, 1024 * 1024) || !valid_text(&a.coverage_note, 64 * 1024) {
        return Err(ArtifactError::Invalid);
    }
    if a.claims.len() > 256
        || a.source_links.len() > 512
        || a.context_links.len() > 512
        || a.workbench_support_links.len() > 512
        || a.assumptions.len() > 128
        || a.alternatives.len() > 128
        || a.evidence_gaps.len() > 128
        || a.clarifications.len() > 64
    {
        return Err(ArtifactError::Invalid);
    }
    let mut ord = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    for c in &a.claims {
        if c.ordinal < 1
            || !valid_kind(&c.kind)
            || !valid_text(&c.text, 8192)
            || c.qualification
                .as_ref()
                .is_some_and(|q| !valid_text(q, 4096))
            || !ord.insert(c.ordinal)
            || !ids.insert(c.claim_id)
        {
            return Err(ArtifactError::Invalid);
        }
    }
    let claim_ids: std::collections::BTreeSet<Uuid> =
        a.claims.iter().map(|claim| claim.claim_id).collect();
    let mut workbench_links = std::collections::BTreeSet::new();
    for link in &a.workbench_support_links {
        if !claim_ids.contains(&link.claim_id) || !workbench_links.insert(link) {
            return Err(ArtifactError::Invalid);
        }
    }
    let mut source_links = std::collections::BTreeSet::new();
    for link in &a.source_links {
        if !claim_ids.contains(&link.claim_id)
            || !matches!(
                link.relation.as_str(),
                "SUPPORTS_AS_USED" | "CONTRADICTS_AS_USED" | "CONTEXT_ONLY"
            )
            || !source_links.insert((link.claim_id, link.evidence_id, link.relation.clone()))
        {
            return Err(ArtifactError::Invalid);
        }
    }
    let mut context_links = std::collections::BTreeSet::new();
    for link in &a.context_links {
        if !claim_ids.contains(&link.claim_id)
            || !matches!(
                link.relation.as_str(),
                "INFORMS" | "QUALIFIES" | "CONFLICTS_WITH_REPORTED_CONTEXT"
            )
            || !context_links.insert((link.claim_id, link.contribution_id, link.relation.clone()))
        {
            return Err(ArtifactError::Invalid);
        }
    }
    let mut a_ids = std::collections::BTreeSet::new();
    for x in &a.assumptions {
        if !a_ids.insert(x.assumption_id)
            || !valid_text(&x.statement, 4096)
            || !valid_text(&x.basis_note, 4096)
        {
            return Err(ArtifactError::Invalid);
        }
    }
    let mut alt_ids = std::collections::BTreeSet::new();
    for x in &a.alternatives {
        if !alt_ids.insert(x.alternative_id)
            || !valid_text(&x.text, 4096)
            || !valid_text(&x.rationale, 4096)
        {
            return Err(ArtifactError::Invalid);
        }
    }
    let mut gap_ids = std::collections::BTreeSet::new();
    for x in &a.evidence_gaps {
        if !gap_ids.insert(x.gap_id)
            || !valid_text(&x.description, 4096)
            || x.requested_source_kind
                .as_ref()
                .is_some_and(|v| v.trim().is_empty() || v.len() > 256)
        {
            return Err(ArtifactError::Invalid);
        }
    }
    let mut q_ids = std::collections::BTreeSet::new();
    for x in &a.clarifications {
        if !q_ids.insert(x.question_id)
            || !valid_text(&x.text, 4096)
            || !valid_text(&x.reason, 2048)
        {
            return Err(ArtifactError::Invalid);
        }
    }
    Ok(())
}

/// Reject synthesis fields that would let a model author hypotheses, what-if
/// assumptions, or candidate possibilities in the restricted MVP.
///
/// Historical artifact structures remain readable for compatibility. The
/// active model runtime calls this stricter boundary before any persistence.
pub fn validate_restricted_mvp_authorship(
    artifact: &SynthesisArtifact,
) -> Result<(), ArtifactError> {
    if !artifact.assumptions.is_empty()
        || !artifact.alternatives.is_empty()
        || artifact.claims.iter().any(|claim| claim.kind == "POSSIBILITY")
    {
        return Err(ArtifactError::HumanAuthorshipRequired);
    }
    Ok(())
}
async fn scoped(pool: &PgPool, w: Uuid) -> Result<Transaction<'_, Postgres>, ArtifactError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
        .execute(&mut *tx)
        .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ArtifactError::Invalid);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(w.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}
pub fn parse(response: &Value) -> Result<Option<SynthesisArtifact>, ArtifactError> {
    let Some(v) = response.get("niccolo_artifact") else {
        return Ok(None);
    };
    let a: SynthesisArtifact = serde_json::from_value(v.clone())?;
    validate(&a)?;
    Ok(Some(a))
}
pub async fn apply(
    pool: &PgPool,
    scope: AnalysisScope,
    actor: Uuid,
    artifact: &SynthesisArtifact,
) -> Result<AppliedArtifact, ArtifactError> {
    validate(artifact)?;
    let mut tx = scoped(pool, scope.workspace).await?;
    let mut workbench_support_links = artifact.workbench_support_links.clone();
    workbench_support_links.sort_unstable();
    if !workbench_support_links.is_empty() {
        let basis = analysis_workbench_support::read_in(&mut tx, scope)
            .await?
            .ok_or(ArtifactError::WorkbenchSupportUnavailable)?;
        if !workbench_support_links.iter().all(|link| basis.allows(link)) {
            return Err(ArtifactError::WorkbenchSupportUnavailable);
        }
    }
    let links_json = serde_json::to_string(&workbench_support_links)?;
    let inserted=sqlx::query("INSERT INTO niccolo.situation_analysis_outputs(workspace_id,situation_id,analysis_id,answer_text,coverage_note,published_by_principal_id,workbench_support_links) VALUES($1,$2,$3,$4,$5,$6,$7::text::jsonb) ON CONFLICT DO NOTHING")
  .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(artifact.answer_text.trim()).bind(artifact.coverage_note.trim()).bind(actor).bind(&links_json).execute(&mut *tx).await?.rows_affected()==1;
    let stored:Option<(String,String,Uuid,bool)>=sqlx::query_as("SELECT answer_text,coverage_note,published_by_principal_id,workbench_support_links=$4::text::jsonb FROM niccolo.situation_analysis_outputs WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(&links_json).fetch_optional(&mut *tx).await?;
    if stored != Some((
        artifact.answer_text.trim().into(),
        artifact.coverage_note.trim().into(),
        actor,
        true,
    ))
    {
        return Err(ArtifactError::Conflict);
    }
    for c in &artifact.claims {
        sqlx::query("INSERT INTO niccolo.situation_analysis_claims(workspace_id,situation_id,analysis_id,claim_id,ordinal,claim_kind,claim_text,qualification,created_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING")
    .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(c.claim_id).bind(c.ordinal).bind(&c.kind).bind(c.text.trim()).bind(c.qualification.as_deref().map(str::trim)).bind(actor).execute(&mut *tx).await?;
        let row:Option<(i32,String,String,Option<String>,Uuid)>=sqlx::query_as("SELECT ordinal,claim_kind,claim_text,qualification,created_by_principal_id FROM niccolo.situation_analysis_claims WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(c.claim_id).fetch_optional(&mut *tx).await?;
        if row
            != Some((
                c.ordinal,
                c.kind.clone(),
                c.text.trim().into(),
                c.qualification.as_deref().map(str::trim).map(str::to_owned),
                actor,
            ))
        {
            return Err(ArtifactError::Conflict);
        }
    }
    for link in &artifact.source_links {
        sqlx::query("INSERT INTO niccolo.situation_analysis_claim_fragments(workspace_id,situation_id,analysis_id,claim_id,evidence_id,relation,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
            .bind(scope.workspace)
            .bind(scope.situation)
            .bind(scope.analysis)
            .bind(link.claim_id)
            .bind(link.evidence_id)
            .bind(&link.relation)
            .bind(actor)
            .execute(&mut *tx)
            .await?;
        let stored: Option<Uuid> = sqlx::query_scalar(
            "SELECT recorded_by_principal_id FROM niccolo.situation_analysis_claim_fragments WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4 AND evidence_id=$5 AND relation=$6",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(link.claim_id)
        .bind(link.evidence_id)
        .bind(&link.relation)
        .fetch_optional(&mut *tx)
        .await?;
        if stored != Some(actor) {
            return Err(ArtifactError::Conflict);
        }
    }
    for link in &artifact.context_links {
        sqlx::query("INSERT INTO niccolo.situation_analysis_claim_contributions(workspace_id,situation_id,analysis_id,claim_id,contribution_id,relation,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
            .bind(scope.workspace)
            .bind(scope.situation)
            .bind(scope.analysis)
            .bind(link.claim_id)
            .bind(link.contribution_id)
            .bind(&link.relation)
            .bind(actor)
            .execute(&mut *tx)
            .await?;
        let stored: Option<Uuid> = sqlx::query_scalar(
            "SELECT recorded_by_principal_id FROM niccolo.situation_analysis_claim_contributions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4 AND contribution_id=$5 AND relation=$6",
        )
        .bind(scope.workspace)
        .bind(scope.situation)
        .bind(scope.analysis)
        .bind(link.claim_id)
        .bind(link.contribution_id)
        .bind(&link.relation)
        .fetch_optional(&mut *tx)
        .await?;
        if stored != Some(actor) {
            return Err(ArtifactError::Conflict);
        }
    }
    for a in &artifact.assumptions {
        sqlx::query("INSERT INTO niccolo.situation_analysis_assumptions(workspace_id,situation_id,analysis_id,assumption_id,claim_id,statement,basis_note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING")
    .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(a.assumption_id).bind(a.claim_id).bind(a.statement.trim()).bind(a.basis_note.trim()).bind(actor).execute(&mut *tx).await?;
        let row:Option<(Option<Uuid>,String,String,Uuid)>=sqlx::query_as("SELECT claim_id,statement,basis_note,recorded_by_principal_id FROM niccolo.situation_analysis_assumptions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND assumption_id=$4").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(a.assumption_id).fetch_optional(&mut *tx).await?;
        if row
            != Some((
                a.claim_id,
                a.statement.trim().into(),
                a.basis_note.trim().into(),
                actor,
            ))
        {
            return Err(ArtifactError::Conflict);
        }
    }
    for a in &artifact.alternatives {
        sqlx::query("INSERT INTO niccolo.situation_analysis_claim_alternatives(workspace_id,situation_id,analysis_id,claim_id,alternative_id,alternative_text,rationale,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING")
    .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(a.claim_id).bind(a.alternative_id).bind(a.text.trim()).bind(a.rationale.trim()).bind(actor).execute(&mut *tx).await?;
        let row:Option<(String,String,Uuid)>=sqlx::query_as("SELECT alternative_text,rationale,recorded_by_principal_id FROM niccolo.situation_analysis_claim_alternatives WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4 AND alternative_id=$5").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(a.claim_id).bind(a.alternative_id).fetch_optional(&mut *tx).await?;
        if row != Some((a.text.trim().into(), a.rationale.trim().into(), actor)) {
            return Err(ArtifactError::Conflict);
        }
    }
    for g in &artifact.evidence_gaps {
        sqlx::query("INSERT INTO niccolo.situation_analysis_evidence_gaps(workspace_id,situation_id,analysis_id,gap_id,claim_id,description,requested_source_kind,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING")
    .bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(g.gap_id).bind(g.claim_id).bind(g.description.trim()).bind(g.requested_source_kind.as_deref().map(str::trim)).bind(actor).execute(&mut *tx).await?;
        let row:Option<(Option<Uuid>,String,Option<String>,Uuid)>=sqlx::query_as("SELECT claim_id,description,requested_source_kind,recorded_by_principal_id FROM niccolo.situation_analysis_evidence_gaps WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND gap_id=$4").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(g.gap_id).fetch_optional(&mut *tx).await?;
        if row
            != Some((
                g.claim_id,
                g.description.trim().into(),
                g.requested_source_kind
                    .as_deref()
                    .map(str::trim)
                    .map(str::to_owned),
                actor,
            ))
        {
            return Err(ArtifactError::Conflict);
        }
    }
    for q in &artifact.clarifications {
        sqlx::query("INSERT INTO niccolo.investigative_questions(workspace_id,question_id,question) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(q.question_id).bind(q.text.trim()).execute(&mut *tx).await?;
        let question:Option<String>=sqlx::query_scalar("SELECT question FROM niccolo.investigative_questions WHERE workspace_id=$1 AND question_id=$2").bind(scope.workspace).bind(q.question_id).fetch_optional(&mut *tx).await?;
        if question.as_deref() != Some(q.text.trim()) {
            return Err(ArtifactError::Conflict);
        }
        sqlx::query("INSERT INTO niccolo.situation_analysis_clarification_questions(workspace_id,situation_id,analysis_id,question_id,reason,proposed_by_principal_id) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(q.question_id).bind(q.reason.trim()).bind(actor).execute(&mut *tx).await?;
        let row:Option<(String,Uuid)>=sqlx::query_as("SELECT reason,proposed_by_principal_id FROM niccolo.situation_analysis_clarification_questions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND question_id=$4").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(q.question_id).fetch_optional(&mut *tx).await?;
        if row != Some((q.reason.trim().into(), actor)) {
            return Err(ArtifactError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(AppliedArtifact {
        claim_count: artifact.claims.len(),
        source_link_count: artifact.source_links.len(),
        context_link_count: artifact.context_links.len(),
        assumption_count: artifact.assumptions.len(),
        alternative_count: artifact.alternatives.len(),
        evidence_gap_count: artifact.evidence_gaps.len(),
        clarification_count: artifact.clarifications.len(),
        replayed: !inserted,
        recorded_output_is_truth: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_artifact() -> SynthesisArtifact {
        SynthesisArtifact {
            answer_text: "Provisional analysis".into(),
            coverage_note: "No parser used".into(),
            claims: vec![],
            source_links: vec![],
            context_links: vec![],
            workbench_support_links: vec![],
            assumptions: vec![],
            alternatives: vec![],
            evidence_gaps: vec![],
            clarifications: vec![],
        }
    }

    #[test]
    fn restricted_synthesis_cannot_smuggle_model_hypotheses_or_assumptions() {
        let claim_id = Uuid::new_v4();
        let mut assumption = base_artifact();
        assumption.assumptions.push(ArtifactAssumption {
            assumption_id: Uuid::new_v4(),
            claim_id: None,
            statement: "What if the person was present?".into(),
            basis_note: "Invented by the model".into(),
        });
        let mut alternative = base_artifact();
        alternative.alternatives.push(ArtifactAlternative {
            alternative_id: Uuid::new_v4(),
            claim_id,
            text: "Another invented explanation".into(),
            rationale: "Model-generated".into(),
        });
        let mut possibility = base_artifact();
        possibility.claims.push(ArtifactClaim {
            claim_id,
            ordinal: 1,
            kind: "POSSIBILITY".into(),
            text: "An invented possibility".into(),
            qualification: Some("Still not human-authored".into()),
        });
        for artifact in [assumption, alternative, possibility] {
            assert!(matches!(
                validate_restricted_mvp_authorship(&artifact),
                Err(ArtifactError::HumanAuthorshipRequired)
            ));
        }
        let mut inference = base_artifact();
        inference.claims.push(ArtifactClaim {
            claim_id,
            ordinal: 1,
            kind: "INFERENCE".into(),
            text: "This follows from the explicitly authored structure".into(),
            qualification: Some("Analysis, not a new hypothesis".into()),
        });
        validate_restricted_mvp_authorship(&inference).unwrap();
    }
}
