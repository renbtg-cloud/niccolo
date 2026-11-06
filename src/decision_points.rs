//! v0.106-v0.111: human-facing decision points and append-only selections.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DecisionError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("invalid decision input")]
    Invalid,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionOption {
    pub option_id: Uuid,
    pub ordinal: i32,
    pub text: String,
    pub tradeoff_note: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionEvent {
    pub event_id: Uuid,
    pub kind: String,
    pub option_id: Option<Uuid>,
    pub note: String,
    pub recorded_by: Uuid,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionView {
    pub option_id: Uuid,
    pub ordinal: i32,
    pub text: String,
    pub tradeoff_note: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionView {
    pub decision_id: Uuid,
    pub question: String,
    pub context_note: Option<String>,
    pub options: Vec<OptionView>,
    pub selected_option_id: Option<Uuid>,
    pub current_state: String,
    pub options_are_automatically_ranked: bool,
    pub selection_means_executed: bool,
    pub selection_means_system_recommendation: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionPackage {
    pub decisions: Vec<DecisionView>,
    pub human_owns_decision: bool,
    pub package_executes_actions: bool,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
    ro: bool,
) -> Result<Transaction<'_, Postgres>, DecisionError> {
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
        return Err(DecisionError::Privileged);
    };
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}
fn valid(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 4096
}
pub async fn create(
    pool: &PgPool,
    scope: AnalysisScope,
    id: Uuid,
    question: &str,
    context: Option<&str>,
    principal: Uuid,
) -> Result<(), DecisionError> {
    if !valid(question) || context.is_some_and(|x| !valid(x)) {
        return Err(DecisionError::Invalid);
    };
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_decision_points(workspace_id,situation_id,analysis_id,decision_id,question,context_note,created_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(id).bind(question).bind(context).bind(principal).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn add_option(
    pool: &PgPool,
    scope: AnalysisScope,
    decision: Uuid,
    opt: &DecisionOption,
    principal: Uuid,
) -> Result<(), DecisionError> {
    if opt.ordinal < 1 || !valid(&opt.text) || opt.tradeoff_note.as_ref().is_some_and(|x| !valid(x))
    {
        return Err(DecisionError::Invalid);
    };
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_decision_options(workspace_id,situation_id,analysis_id,decision_id,option_id,ordinal,option_text,tradeoff_note,created_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(decision).bind(opt.option_id).bind(opt.ordinal).bind(&opt.text).bind(&opt.tradeoff_note).bind(principal).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn append_selection(
    pool: &PgPool,
    scope: AnalysisScope,
    decision: Uuid,
    event: &SelectionEvent,
) -> Result<(), DecisionError> {
    if !matches!(event.kind.as_str(), "SELECTED" | "CLEARED" | "REOPENED")
        || !valid(&event.note)
        || (event.kind == "SELECTED") != event.option_id.is_some()
    {
        return Err(DecisionError::Invalid);
    };
    let mut tx = scoped(pool, scope.workspace, false).await?;
    sqlx::query("INSERT INTO niccolo.situation_analysis_decision_selection_events(workspace_id,situation_id,analysis_id,decision_id,event_id,event_kind,option_id,note,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(decision).bind(event.event_id).bind(&event.kind).bind(event.option_id).bind(&event.note).bind(event.recorded_by).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}
pub async fn package(
    pool: &PgPool,
    scope: AnalysisScope,
) -> Result<DecisionPackage, DecisionError> {
    let mut tx = scoped(pool, scope.workspace, true).await?;
    let ds:Vec<(Uuid,String,Option<String>)>=sqlx::query_as("SELECT decision_id,question,context_note FROM niccolo.situation_analysis_decision_points WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 ORDER BY decision_id").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).fetch_all(&mut *tx).await?;
    let mut decisions = Vec::with_capacity(ds.len());
    for (decision_id, question, context_note) in ds {
        let os:Vec<(Uuid,i32,String,Option<String>)>=sqlx::query_as("SELECT option_id,ordinal,option_text,tradeoff_note FROM niccolo.situation_analysis_decision_options WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND decision_id=$4 ORDER BY ordinal,option_id").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(decision_id).fetch_all(&mut *tx).await?;
        let latest:Option<(String,Option<Uuid>)>=sqlx::query_as("SELECT event_kind,option_id FROM niccolo.situation_analysis_decision_selection_events WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND decision_id=$4 ORDER BY created_at DESC,event_id DESC LIMIT 1").bind(scope.workspace).bind(scope.situation).bind(scope.analysis).bind(decision_id).fetch_optional(&mut *tx).await?;
        let (current_state, selected_option_id) = latest.unwrap_or(("OPEN".into(), None));
        decisions.push(DecisionView {
            decision_id,
            question,
            context_note,
            options: os
                .into_iter()
                .map(|(option_id, ordinal, text, tradeoff_note)| OptionView {
                    option_id,
                    ordinal,
                    text,
                    tradeoff_note,
                })
                .collect(),
            selected_option_id,
            current_state,
            options_are_automatically_ranked: false,
            selection_means_executed: false,
            selection_means_system_recommendation: false,
        });
    }
    tx.commit().await?;
    Ok(DecisionPackage {
        decisions,
        human_owns_decision: true,
        package_executes_actions: false,
    })
}
pub fn canonical_json(v: &DecisionPackage) -> Result<String, serde_json::Error> {
    serde_json::to_string(v)
}
pub fn sha256(v: &DecisionPackage) -> Result<String, serde_json::Error> {
    Ok(format!(
        "{:x}",
        Sha256::digest(canonical_json(v)?.as_bytes())
    ))
}
