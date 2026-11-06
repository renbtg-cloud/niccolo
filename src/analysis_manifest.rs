//! v0.36: deterministic, immutable analysis bundle manifests.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("analysis output is not visible")]
    Invisible,
    #[error("manifest idempotency conflict")]
    Conflict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleClaim {
    pub claim_id: Uuid,
    pub ordinal: i32,
    pub kind: String,
    pub text: String,
    pub qualification: Option<String>,
    pub created_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleFragmentLink {
    pub claim_id: Uuid,
    pub evidence_id: Uuid,
    pub relation: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleContributionLink {
    pub claim_id: Uuid,
    pub contribution_id: Uuid,
    pub relation: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleReview {
    pub claim_id: Uuid,
    pub review_id: Uuid,
    pub kind: String,
    pub note: String,
    pub reviewer: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleLineage {
    pub lineage_id: Uuid,
    pub prior_analysis_id: Uuid,
    pub prior_claim_id: Uuid,
    pub new_claim_id: Option<Uuid>,
    pub relation: String,
    pub rationale: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleImpact {
    pub impact_id: Uuid,
    pub source_analysis_id: Uuid,
    pub question_id: Uuid,
    pub answer_id: Uuid,
    pub contribution_id: Uuid,
    pub target_claim_id: Uuid,
    pub recorded_effect: String,
    pub note: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleQuestion {
    pub question_id: Uuid,
    pub reason: String,
    pub proposed_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleAnswer {
    pub question_id: Uuid,
    pub answer_id: Uuid,
    pub contribution_id: Uuid,
    pub answered_by: Uuid,
    pub answer_revision: i64,
}

type HeaderRow = (
    i64,
    String,
    i64,
    String,
    Uuid,
    Option<Uuid>,
    String,
    String,
    Uuid,
);
type ImpactRow = (Uuid, Uuid, Uuid, Uuid, Uuid, Uuid, String, String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisBundle {
    pub bundle_version: u32,
    pub workspace_id: Uuid,
    pub situation_id: Uuid,
    pub analysis_id: Uuid,
    pub input_revision: i64,
    pub input_fingerprint: String,
    pub identity_event_no_max: i64,
    pub goal: String,
    pub requested_by: Uuid,
    pub reanalysis_of: Option<Uuid>,
    pub answer_text: String,
    pub coverage_note: String,
    pub published_by: Uuid,
    pub examined_fragments: Vec<Uuid>,
    pub used_contributions: Vec<Uuid>,
    pub clarification_questions: Vec<BundleQuestion>,
    pub clarification_answers: Vec<BundleAnswer>,
    pub claims: Vec<BundleClaim>,
    pub claim_fragment_links: Vec<BundleFragmentLink>,
    pub claim_contribution_links: Vec<BundleContributionLink>,
    pub claim_reviews: Vec<BundleReview>,
    pub incoming_claim_lineage: Vec<BundleLineage>,
    pub clarification_impacts: Vec<BundleImpact>,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, ManifestError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if privileged {
        return Err(ManifestError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn load_bundle(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<AnalysisBundle, ManifestError> {
    let mut tx = scoped(pool, workspace).await?;
    let header: Option<HeaderRow> = sqlx::query_as(
        "SELECT s.input_revision,s.input_fingerprint,s.identity_event_no_max,s.goal,s.requested_by_principal_id,s.reanalysis_of,\
                o.answer_text,o.coverage_note,o.published_by_principal_id \
         FROM niccolo.situation_analysis_snapshots s \
         JOIN niccolo.situation_analysis_outputs o USING(workspace_id,situation_id,analysis_id) \
         WHERE s.workspace_id=$1 AND s.situation_id=$2 AND s.analysis_id=$3",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((
        input_revision,
        input_fingerprint,
        identity_event_no_max,
        goal,
        requested_by,
        reanalysis_of,
        answer_text,
        coverage_note,
        published_by,
    )) = header
    else {
        return Err(ManifestError::Invisible);
    };

    let examined_fragments: Vec<Uuid> = sqlx::query_scalar(
        "SELECT evidence_id FROM niccolo.situation_analysis_examined_fragments \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 ORDER BY evidence_id",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_all(&mut *tx)
    .await?;
    let used_contributions: Vec<Uuid> = sqlx::query_scalar(
        "SELECT contribution_id FROM niccolo.situation_analysis_used_contributions \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 ORDER BY contribution_id",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .fetch_all(&mut *tx)
    .await?;

    let qrows: Vec<(Uuid,String,Uuid)> = sqlx::query_as(
        "SELECT question_id,reason,proposed_by_principal_id FROM niccolo.situation_analysis_clarification_questions \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 ORDER BY question_id",
    ).bind(workspace).bind(situation).bind(analysis).fetch_all(&mut *tx).await?;
    let clarification_questions = qrows
        .into_iter()
        .map(|(question_id, reason, proposed_by)| BundleQuestion {
            question_id,
            reason,
            proposed_by,
        })
        .collect();

    let arows: Vec<(Uuid,Uuid,Uuid,Uuid,i64)> = sqlx::query_as(
        "SELECT question_id,answer_id,contribution_id,answered_by_principal_id,answer_revision \
         FROM niccolo.situation_analysis_clarification_answers \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 ORDER BY question_id,answer_id",
    ).bind(workspace).bind(situation).bind(analysis).fetch_all(&mut *tx).await?;
    let clarification_answers = arows
        .into_iter()
        .map(
            |(question_id, answer_id, contribution_id, answered_by, answer_revision)| {
                BundleAnswer {
                    question_id,
                    answer_id,
                    contribution_id,
                    answered_by,
                    answer_revision,
                }
            },
        )
        .collect();

    let crows: Vec<(Uuid,i32,String,String,Option<String>,Uuid)> = sqlx::query_as(
        "SELECT claim_id,ordinal,claim_kind,claim_text,qualification,created_by_principal_id \
         FROM niccolo.situation_analysis_claims WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 \
         ORDER BY ordinal,claim_id",
    ).bind(workspace).bind(situation).bind(analysis).fetch_all(&mut *tx).await?;
    let claims = crows
        .into_iter()
        .map(
            |(claim_id, ordinal, kind, text, qualification, created_by)| BundleClaim {
                claim_id,
                ordinal,
                kind,
                text,
                qualification,
                created_by,
            },
        )
        .collect();

    let frows: Vec<(Uuid,Uuid,String)> = sqlx::query_as(
        "SELECT claim_id,evidence_id,relation FROM niccolo.situation_analysis_claim_fragments \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 ORDER BY claim_id,evidence_id,relation",
    ).bind(workspace).bind(situation).bind(analysis).fetch_all(&mut *tx).await?;
    let claim_fragment_links = frows
        .into_iter()
        .map(|(claim_id, evidence_id, relation)| BundleFragmentLink {
            claim_id,
            evidence_id,
            relation,
        })
        .collect();

    let urows: Vec<(Uuid,Uuid,String)> = sqlx::query_as(
        "SELECT claim_id,contribution_id,relation FROM niccolo.situation_analysis_claim_contributions \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 ORDER BY claim_id,contribution_id,relation",
    ).bind(workspace).bind(situation).bind(analysis).fetch_all(&mut *tx).await?;
    let claim_contribution_links = urows
        .into_iter()
        .map(
            |(claim_id, contribution_id, relation)| BundleContributionLink {
                claim_id,
                contribution_id,
                relation,
            },
        )
        .collect();

    let rrows: Vec<(Uuid,Uuid,String,String,Uuid)> = sqlx::query_as(
        "SELECT claim_id,review_id,review_kind,note,reviewer_principal_id FROM niccolo.situation_analysis_claim_reviews \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 ORDER BY claim_id,review_id",
    ).bind(workspace).bind(situation).bind(analysis).fetch_all(&mut *tx).await?;
    let claim_reviews = rrows
        .into_iter()
        .map(|(claim_id, review_id, kind, note, reviewer)| BundleReview {
            claim_id,
            review_id,
            kind,
            note,
            reviewer,
        })
        .collect();

    let lrows: Vec<(Uuid,Uuid,Uuid,Option<Uuid>,String,String)> = sqlx::query_as(
        "SELECT lineage_id,prior_analysis_id,prior_claim_id,new_claim_id,relation,rationale \
         FROM niccolo.situation_analysis_claim_lineage \
         WHERE workspace_id=$1 AND situation_id=$2 AND new_analysis_id=$3 ORDER BY prior_claim_id,lineage_id",
    ).bind(workspace).bind(situation).bind(analysis).fetch_all(&mut *tx).await?;
    let incoming_claim_lineage = lrows
        .into_iter()
        .map(
            |(lineage_id, prior_analysis_id, prior_claim_id, new_claim_id, relation, rationale)| {
                BundleLineage {
                    lineage_id,
                    prior_analysis_id,
                    prior_claim_id,
                    new_claim_id,
                    relation,
                    rationale,
                }
            },
        )
        .collect();

    let irows: Vec<ImpactRow> = sqlx::query_as(
        "SELECT impact_id,source_analysis_id,question_id,answer_id,contribution_id,target_claim_id,recorded_effect,note \
         FROM niccolo.situation_analysis_clarification_claim_impacts \
         WHERE workspace_id=$1 AND situation_id=$2 AND target_analysis_id=$3 ORDER BY impact_id",
    ).bind(workspace).bind(situation).bind(analysis).fetch_all(&mut *tx).await?;
    let clarification_impacts = irows
        .into_iter()
        .map(
            |(
                impact_id,
                source_analysis_id,
                question_id,
                answer_id,
                contribution_id,
                target_claim_id,
                recorded_effect,
                note,
            )| BundleImpact {
                impact_id,
                source_analysis_id,
                question_id,
                answer_id,
                contribution_id,
                target_claim_id,
                recorded_effect,
                note,
            },
        )
        .collect();

    tx.commit().await?;
    Ok(AnalysisBundle {
        bundle_version: 1,
        workspace_id: workspace,
        situation_id: situation,
        analysis_id: analysis,
        input_revision,
        input_fingerprint,
        identity_event_no_max,
        goal,
        requested_by,
        reanalysis_of,
        answer_text,
        coverage_note,
        published_by,
        examined_fragments,
        used_contributions,
        clarification_questions,
        clarification_answers,
        claims,
        claim_fragment_links,
        claim_contribution_links,
        claim_reviews,
        incoming_claim_lineage,
        clarification_impacts,
    })
}

#[derive(Serialize)]
struct ManifestCore<'a> {
    manifest_core_version: u32,
    workspace_id: Uuid,
    situation_id: Uuid,
    analysis_id: Uuid,
    input_revision: i64,
    input_fingerprint: &'a str,
    identity_event_no_max: i64,
    goal: &'a str,
    requested_by: Uuid,
    reanalysis_of: Option<Uuid>,
    answer_text: &'a str,
    coverage_note: &'a str,
    published_by: Uuid,
    examined_fragments: &'a [Uuid],
    used_contributions: &'a [Uuid],
    clarification_questions: &'a [BundleQuestion],
    claims: &'a [BundleClaim],
    claim_fragment_links: &'a [BundleFragmentLink],
    claim_contribution_links: &'a [BundleContributionLink],
}

pub fn canonical_json(bundle: &AnalysisBundle) -> Result<Vec<u8>, ManifestError> {
    // Human reviews, clarification answers, reanalysis lineage and later impact records are
    // intentionally excluded: they are append-only events that may legitimately arrive after
    // the analysis itself has been sealed.
    let core = ManifestCore {
        manifest_core_version: 2,
        workspace_id: bundle.workspace_id,
        situation_id: bundle.situation_id,
        analysis_id: bundle.analysis_id,
        input_revision: bundle.input_revision,
        input_fingerprint: &bundle.input_fingerprint,
        identity_event_no_max: bundle.identity_event_no_max,
        goal: &bundle.goal,
        requested_by: bundle.requested_by,
        reanalysis_of: bundle.reanalysis_of,
        answer_text: &bundle.answer_text,
        coverage_note: &bundle.coverage_note,
        published_by: bundle.published_by,
        examined_fragments: &bundle.examined_fragments,
        used_contributions: &bundle.used_contributions,
        clarification_questions: &bundle.clarification_questions,
        claims: &bundle.claims,
        claim_fragment_links: &bundle.claim_fragment_links,
        claim_contribution_links: &bundle.claim_contribution_links,
    };
    Ok(serde_json::to_vec(&core)?)
}

fn hex(bytes: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(H[(b >> 4) as usize] as char);
        out.push(H[(b & 0x0f) as usize] as char);
    }
    out
}

pub fn hash_bundle(bundle: &AnalysisBundle) -> Result<String, ManifestError> {
    let bytes = canonical_json(bundle)?;
    Ok(hex(&Sha256::digest(bytes)))
}

pub async fn compute_hash(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<String, ManifestError> {
    hash_bundle(&load_bundle(pool, workspace, situation, analysis).await?)
}

pub async fn record_manifest(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
    principal: Uuid,
) -> Result<String, ManifestError> {
    let digest = compute_hash(pool, workspace, situation, analysis).await?;
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
        return Err(ManifestError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    sqlx::query(
        "INSERT INTO niccolo.situation_analysis_manifests \
         (workspace_id,situation_id,analysis_id,manifest_version,manifest_sha256,recorded_by_principal_id) \
         VALUES($1,$2,$3,1,$4,$5) ON CONFLICT DO NOTHING",
    ).bind(workspace).bind(situation).bind(analysis).bind(&digest).bind(principal).execute(&mut *tx).await?;
    let stored: Option<(i32,String,Uuid)> = sqlx::query_as(
        "SELECT manifest_version,manifest_sha256,recorded_by_principal_id FROM niccolo.situation_analysis_manifests \
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    ).bind(workspace).bind(situation).bind(analysis).fetch_optional(&mut *tx).await?;
    let Some((version, stored_digest, stored_by)) = stored else {
        return Err(ManifestError::Invisible);
    };
    if version != 1 || stored_digest != digest || stored_by != principal {
        return Err(ManifestError::Conflict);
    }
    tx.commit().await?;
    Ok(digest)
}
