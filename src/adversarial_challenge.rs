//! v0.901-v1.000: adversarial self-challenge persisted as inspectable criticism.
//! Materiality is a scheduling/review heuristic, not a truth probability.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
#[derive(Debug, thiserror::Error)]
pub enum ChallengeError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid challenge artifact")]
    Invalid,
    #[error("challenge replay conflict")]
    Conflict,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeItem {
    pub challenge_id: Uuid,
    pub kind: String,
    pub text: String,
    pub materiality: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeArtifact {
    #[serde(default)]
    pub challenges: Vec<ChallengeItem>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChallengeView {
    pub challenge_id: Uuid,
    pub kind: String,
    pub text: String,
    pub materiality: i32,
    pub latest_event: Option<String>,
    pub open: bool,
    pub materiality_is_truth_probability: bool,
    pub challenge_establishes_error: bool,
}
fn validate(a: &ChallengeArtifact) -> Result<(), ChallengeError> {
    if a.challenges.len() > 64 {
        return Err(ChallengeError::Invalid);
    };
    let mut ids = std::collections::BTreeSet::new();
    for c in &a.challenges {
        if !ids.insert(c.challenge_id)
            || !matches!(
                c.kind.as_str(),
                "COUNTEREVIDENCE" | "ALTERNATIVE" | "ASSUMPTION_RISK" | "COVERAGE_GAP" | "OTHER"
            )
            || c.text.trim().is_empty()
            || c.text.len() > 4096
            || c.text.contains('\0')
            || !(0..=100).contains(&c.materiality)
        {
            return Err(ChallengeError::Invalid);
        }
    }
    Ok(())
}
async fn scoped(
    pool: &PgPool,
    w: Uuid,
    ro: bool,
) -> Result<Transaction<'_, Postgres>, ChallengeError> {
    let mut tx = pool.begin().await?;
    sqlx::query(if ro {
        "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
    } else {
        "SET TRANSACTION ISOLATION LEVEL READ COMMITTED"
    })
    .execute(&mut *tx)
    .await?;
    let p: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await?;
    if p {
        return Err(ChallengeError::Invalid);
    };
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(w.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}
pub fn parse(v: &Value) -> Result<Option<ChallengeArtifact>, ChallengeError> {
    let Some(x) = v.get("niccolo_challenge") else {
        return Ok(None);
    };
    let x: ChallengeArtifact = serde_json::from_value(x.clone())?;
    validate(&x)?;
    Ok(Some(x))
}
pub async fn apply(
    pool: &PgPool,
    s: AnalysisScope,
    actor: Uuid,
    a: &ChallengeArtifact,
) -> Result<usize, ChallengeError> {
    validate(a)?;
    let mut tx = scoped(pool, s.workspace, false).await?;
    for c in &a.challenges {
        sqlx::query("INSERT INTO niccolo.situation_analysis_challenges(workspace_id,situation_id,analysis_id,challenge_id,challenge_kind,challenge_text,materiality,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING").bind(s.workspace).bind(s.situation).bind(s.analysis).bind(c.challenge_id).bind(&c.kind).bind(c.text.trim()).bind(c.materiality).bind(actor).execute(&mut *tx).await?;
        let row:Option<(String,String,i32,Uuid)>=sqlx::query_as("SELECT challenge_kind,challenge_text,materiality,recorded_by_principal_id FROM niccolo.situation_analysis_challenges WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND challenge_id=$4").bind(s.workspace).bind(s.situation).bind(s.analysis).bind(c.challenge_id).fetch_optional(&mut *tx).await?;
        if row != Some((c.kind.clone(), c.text.trim().into(), c.materiality, actor)) {
            return Err(ChallengeError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(a.challenges.len())
}
pub async fn event(
    pool: &PgPool,
    s: AnalysisScope,
    challenge: Uuid,
    event_id: Uuid,
    kind: &str,
    note: &str,
    actor: Uuid,
) -> Result<(), ChallengeError> {
    if !matches!(kind, "ACKNOWLEDGED" | "WAIVED" | "ADDRESSED" | "REOPENED")
        || note.trim().is_empty()
        || note.len() > 4096
    {
        return Err(ChallengeError::Invalid);
    };
    let mut tx = scoped(pool, s.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_challenge_events(workspace_id,situation_id,analysis_id,challenge_id,event_id,event_kind,note,actor_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(s.workspace).bind(s.situation).bind(s.analysis).bind(challenge).bind(event_id).bind(kind).bind(note.trim()).bind(actor).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn list(pool: &PgPool, s: AnalysisScope) -> Result<Vec<ChallengeView>, ChallengeError> {
    let mut tx = scoped(pool, s.workspace, true).await?;
    let rows:Vec<(Uuid,String,String,i32,Option<String>)>=sqlx::query_as("SELECT c.challenge_id,c.challenge_kind,c.challenge_text,c.materiality,(SELECT e.event_kind FROM niccolo.situation_analysis_challenge_events e WHERE e.workspace_id=c.workspace_id AND e.situation_id=c.situation_id AND e.analysis_id=c.analysis_id AND e.challenge_id=c.challenge_id ORDER BY e.created_at DESC,e.event_id DESC LIMIT 1) FROM niccolo.situation_analysis_challenges c WHERE c.workspace_id=$1 AND c.situation_id=$2 AND c.analysis_id=$3 ORDER BY c.materiality DESC,c.challenge_id").bind(s.workspace).bind(s.situation).bind(s.analysis).fetch_all(&mut *tx).await?;
    tx.commit().await?;
    Ok(rows
        .into_iter()
        .map(|(id, k, t, m, e)| {
            let open = !matches!(e.as_deref(), Some("WAIVED" | "ADDRESSED"));
            ChallengeView {
                challenge_id: id,
                kind: k,
                text: t,
                materiality: m,
                latest_event: e,
                open,
                materiality_is_truth_probability: false,
                challenge_establishes_error: false,
            }
        })
        .collect())
}
