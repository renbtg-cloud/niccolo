//! Read-only screening of an investigator-selected possible origin of a detail.
use super::{
    WorkbenchError,
    public_prominence::{self, Observation, calendar_date},
    scoped,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DetailSource {
    object_id: Uuid,
    representation_id: Uuid,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    witness_actor_id: Uuid,
    testimony_detail: DetailSource,
    candidate_source: DetailSource,
    exposure_record_id: Uuid,
    testimony_from: String,
    testimony_through: String,
    known_through: String,
}

#[derive(PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Disposition {
    ReviewCandidate,
    ExposureAfterTestimony,
    OrderUncertain,
    ExposureNotAvailableAtCutoff,
}

#[derive(Serialize)]
struct AffectedRoute {
    argument_id: Uuid,
    grounding_set_id: Uuid,
    conclusion_object_id: Uuid,
}

#[derive(Serialize)]
pub struct Review {
    witness_actor_id: Uuid,
    testimony_detail: DetailSource,
    testimony_from: String,
    testimony_through: String,
    known_through: String,
    candidate_source: Option<DetailSource>,
    exposure: Option<public_prominence::Record>,
    disposition: Disposition,
    needs_finer_timing: bool,
    detail_needs_review: bool,
    affected_routes: Vec<AffectedRoute>,
}

impl Review {
    pub(super) fn flags_route(&self, argument_id: Uuid, grounding_set_id: Uuid) -> bool {
        self.affected_routes.iter().any(|route| {
            route.argument_id == argument_id && route.grounding_set_id == grounding_set_id
        })
    }
}

pub async fn review(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    input: Input,
) -> Result<Review, WorkbenchError> {
    let mut tx = scoped(pool, workspace, true).await?;
    let result = review_in(&mut tx, workspace, situation, &input).await?;
    tx.commit().await?;
    Ok(result)
}

pub(super) async fn review_in(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    input: &Input,
) -> Result<Review, WorkbenchError> {
    if !calendar_date(&input.testimony_from)
        || !calendar_date(&input.testimony_through)
        || !calendar_date(&input.known_through)
        || input.testimony_from > input.testimony_through
        || input.testimony_through > input.known_through
        || input.testimony_detail.object_id == input.candidate_source.object_id
    {
        return Err(WorkbenchError::Invalid);
    }
    let exposure =
        public_prominence::find_record(tx, workspace, situation, input.exposure_record_id)
            .await?
            .ok_or(WorkbenchError::Invisible)?;
    if exposure.input.audience_actor_id != input.witness_actor_id
        || exposure.input.observation != Observation::ExposureReported
    {
        return Err(WorkbenchError::Invalid);
    }

    let linked: bool = sqlx::query_scalar(
        "SELECT count(*) = 2 FROM niccolo.workbench_artifact_provenance
         WHERE workspace_id=$1 AND situation_id=$2
           AND (object_id, representation_id) IN (($3,$4), ($5,$6))",
    )
    .bind(workspace)
    .bind(situation)
    .bind(input.testimony_detail.object_id)
    .bind(input.testimony_detail.representation_id)
    .bind(input.candidate_source.object_id)
    .bind(input.candidate_source.representation_id)
    .fetch_one(&mut **tx)
    .await?;
    if !linked {
        return Err(WorkbenchError::Invisible);
    }

    let available = exposure.input.available_on <= input.known_through;
    let disposition = if !available {
        Disposition::ExposureNotAvailableAtCutoff
    } else if exposure.input.effective_through < input.testimony_from {
        Disposition::ReviewCandidate
    } else if exposure.input.effective_from > input.testimony_through {
        Disposition::ExposureAfterTestimony
    } else {
        Disposition::OrderUncertain
    };

    let detail_needs_review = disposition == Disposition::ReviewCandidate;
    let needs_finer_timing = disposition == Disposition::OrderUncertain;
    let affected_routes = if detail_needs_review {
        let rows: Vec<(Uuid, Uuid, Uuid)> = sqlx::query_as(
            "WITH RECURSIVE edges AS (
               SELECT DISTINCT m.ground_object_id, a.conclusion_object_id, m.argument_id, m.grounding_set_id
               FROM niccolo.epistemic_grounding_members m
               JOIN niccolo.epistemic_arguments a USING (workspace_id, argument_id)
               WHERE a.workspace_id=$1 AND a.situation_id=$2
                 AND m.member_role <> 'CONTEXT' AND m.ground_object_id IS NOT NULL
             ), reached(object_id) AS (
               SELECT $3::uuid
               UNION
               SELECT e.conclusion_object_id FROM reached r JOIN edges e ON e.ground_object_id=r.object_id
             )
             SELECT DISTINCT e.argument_id, e.grounding_set_id, e.conclusion_object_id
             FROM edges e JOIN reached r ON e.ground_object_id=r.object_id
             ORDER BY e.conclusion_object_id,e.argument_id,e.grounding_set_id",
        )
        .bind(workspace)
        .bind(situation)
        .bind(input.testimony_detail.object_id)
        .fetch_all(&mut **tx)
        .await?;
        rows.into_iter()
            .map(
                |(argument_id, grounding_set_id, conclusion_object_id)| AffectedRoute {
                    argument_id,
                    grounding_set_id,
                    conclusion_object_id,
                },
            )
            .collect()
    } else {
        Vec::new()
    };
    Ok(Review {
        witness_actor_id: input.witness_actor_id,
        testimony_detail: input.testimony_detail.clone(),
        testimony_from: input.testimony_from.clone(),
        testimony_through: input.testimony_through.clone(),
        known_through: input.known_through.clone(),
        candidate_source: available.then(|| input.candidate_source.clone()),
        exposure: available.then_some(exposure),
        disposition,
        needs_finer_timing,
        detail_needs_review,
        affected_routes,
    })
}
