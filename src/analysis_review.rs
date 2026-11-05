//! Compact human-review read model assembled from one repeatable-read snapshot.
//! Counts describe recorded state; they do not establish truth or completeness.
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum AnalysisReviewError {
    #[error("database unavailable: {0}")]
    Database(#[from] sqlx::Error),
    #[error("privileged SQL login prohibited")]
    Privileged,
    #[error("analysis snapshot is not visible")]
    Invisible,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisReview {
    pub analysis_id: Uuid,
    pub goal: String,
    pub requested_by: Uuid,
    pub reanalysis_of: Option<Uuid>,
    pub input_revision: i64,
    pub current_revision: i64,
    pub input_is_current: bool,
    pub focus_changed: bool,
    pub output_present: bool,
    pub input_source_count: i64,
    pub catalogued_fragment_count: i64,
    pub examined_fragment_count: i64,
    pub input_context_count: i64,
    pub used_context_count: i64,
    pub clarification_question_count: i64,
    pub answered_question_count: i64,
    pub clarification_answer_count: i64,
    pub added_source_count: i64,
    pub removed_source_count: i64,
    pub added_context_count: i64,
    pub removed_context_count: i64,
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, AnalysisReviewError> {
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
        return Err(AnalysisReviewError::Privileged);
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await?;
    crate::authz::bind_auth_digest(&mut tx).await?;
    Ok(tx)
}

pub async fn get(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<AnalysisReview, AnalysisReviewError> {
    let mut tx = scoped(pool, workspace).await?;
    let header:Option<(i64,String,Uuid,Option<Uuid>,i64,bool)>=sqlx::query_as(
        "SELECT a.input_revision,a.goal,a.requested_by_principal_id,a.reanalysis_of,s.current_revision,\
                ir.focus<>cr.focus \
         FROM niccolo.situation_analysis_snapshots a \
         JOIN niccolo.situations s USING(workspace_id,situation_id) \
         JOIN niccolo.situation_revisions ir ON ir.workspace_id=a.workspace_id AND ir.situation_id=a.situation_id AND ir.revision=a.input_revision \
         JOIN niccolo.situation_revisions cr ON cr.workspace_id=s.workspace_id AND cr.situation_id=s.situation_id AND cr.revision=s.current_revision \
         WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.analysis_id=$3")
        .bind(workspace).bind(situation).bind(analysis).fetch_optional(&mut *tx).await?;
    let Some((input_revision, goal, requested_by, reanalysis_of, current_revision, focus_changed)) =
        header
    else {
        return Err(AnalysisReviewError::Invisible);
    };
    let output_present:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM niccolo.situation_analysis_outputs WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3)").bind(workspace).bind(situation).bind(analysis).fetch_one(&mut *tx).await?;
    let input_source_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_revision_sources WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3").bind(workspace).bind(situation).bind(input_revision).fetch_one(&mut *tx).await?;
    let catalogued_fragment_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.source_fragments f JOIN niccolo.situation_revision_sources rs ON rs.workspace_id=f.workspace_id AND rs.document_id=f.document_id WHERE rs.workspace_id=$1 AND rs.situation_id=$2 AND rs.revision=$3").bind(workspace).bind(situation).bind(input_revision).fetch_one(&mut *tx).await?;
    let examined_fragment_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_examined_fragments WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(workspace).bind(situation).bind(analysis).fetch_one(&mut *tx).await?;
    let input_context_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_revision_contributions WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3").bind(workspace).bind(situation).bind(input_revision).fetch_one(&mut *tx).await?;
    let used_context_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_used_contributions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(workspace).bind(situation).bind(analysis).fetch_one(&mut *tx).await?;
    let clarification_question_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_clarification_questions WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(workspace).bind(situation).bind(analysis).fetch_one(&mut *tx).await?;
    let clarification_answer_count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_clarification_answers WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(workspace).bind(situation).bind(analysis).fetch_one(&mut *tx).await?;
    let answered_question_count:i64=sqlx::query_scalar("SELECT count(DISTINCT question_id) FROM niccolo.situation_analysis_clarification_answers WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3").bind(workspace).bind(situation).bind(analysis).fetch_one(&mut *tx).await?;
    let added_source_count:i64=sqlx::query_scalar("SELECT count(*) FROM (SELECT document_id FROM niccolo.situation_revision_sources WHERE workspace_id=$1 AND situation_id=$2 AND revision=$4 EXCEPT SELECT document_id FROM niccolo.situation_revision_sources WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3) x").bind(workspace).bind(situation).bind(input_revision).bind(current_revision).fetch_one(&mut *tx).await?;
    let removed_source_count:i64=sqlx::query_scalar("SELECT count(*) FROM (SELECT document_id FROM niccolo.situation_revision_sources WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3 EXCEPT SELECT document_id FROM niccolo.situation_revision_sources WHERE workspace_id=$1 AND situation_id=$2 AND revision=$4) x").bind(workspace).bind(situation).bind(input_revision).bind(current_revision).fetch_one(&mut *tx).await?;
    let added_context_count:i64=sqlx::query_scalar("SELECT count(*) FROM (SELECT contribution_id FROM niccolo.situation_revision_contributions WHERE workspace_id=$1 AND situation_id=$2 AND revision=$4 EXCEPT SELECT contribution_id FROM niccolo.situation_revision_contributions WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3) x").bind(workspace).bind(situation).bind(input_revision).bind(current_revision).fetch_one(&mut *tx).await?;
    let removed_context_count:i64=sqlx::query_scalar("SELECT count(*) FROM (SELECT contribution_id FROM niccolo.situation_revision_contributions WHERE workspace_id=$1 AND situation_id=$2 AND revision=$3 EXCEPT SELECT contribution_id FROM niccolo.situation_revision_contributions WHERE workspace_id=$1 AND situation_id=$2 AND revision=$4) x").bind(workspace).bind(situation).bind(input_revision).bind(current_revision).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(AnalysisReview {
        analysis_id: analysis,
        goal,
        requested_by,
        reanalysis_of,
        input_revision,
        current_revision,
        input_is_current: input_revision == current_revision,
        focus_changed,
        output_present,
        input_source_count,
        catalogued_fragment_count,
        examined_fragment_count,
        input_context_count,
        used_context_count,
        clarification_question_count,
        answered_question_count,
        clarification_answer_count,
        added_source_count,
        removed_source_count,
        added_context_count,
        removed_context_count,
    })
}
