//! Read-only screening of an investigator-selected possible origin of a detail.
use super::{
    WorkbenchError,
    exposure_order::{self, Direction},
    public_prominence::{self, Observation, calendar_date, calendar_range},
    route_use, scoped,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DetailSource {
    pub(super) object_id: Uuid,
    pub(super) representation_id: Uuid,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    witness_actor_id: Uuid,
    pub(super) testimony_detail: DetailSource,
    pub(super) candidate_source: DetailSource,
    pub(super) exposure_record_id: Uuid,
    testimony_from: Option<String>,
    testimony_through: Option<String>,
    pub(super) known_through: String,
}

#[derive(PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Disposition {
    ReviewCandidate,
    ExposureAfterTestimony,
    OrderUncertain,
    OrderConflict,
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
    testimony_from: Option<String>,
    testimony_through: Option<String>,
    known_through: String,
    candidate_source: Option<DetailSource>,
    exposure: Option<public_prominence::Record>,
    date_order: Option<Direction>,
    order_reports: Vec<exposure_order::Report>,
    order_reports_truncated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    withdrawn_order_reports: Vec<exposure_order::WithdrawnReport>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    reconsiderable_suspensions: Vec<route_use::ReconsiderableSuspension>,
    disposition: Disposition,
    needs_finer_timing: bool,
    detail_needs_review: bool,
    affected_routes: Vec<AffectedRoute>,
}

impl Review {
    pub fn withdrawable_report_ids(&self, principal: Uuid) -> Vec<Uuid> {
        self.order_reports
            .iter()
            .filter(|report| report.recorded_by == principal)
            .map(|report| report.report_id)
            .collect()
    }

    pub fn with_sha256(&self) -> Result<serde_json::Value, WorkbenchError> {
        let mut value = serde_json::to_value(self).map_err(|_| WorkbenchError::Invalid)?;
        let bytes = serde_json::to_vec(&value).map_err(|_| WorkbenchError::Invalid)?;
        value["review_sha256"] = serde_json::json!(format!("{:x}", Sha256::digest(bytes)));
        Ok(value)
    }

    pub(super) fn flags_route(&self, argument_id: Uuid, grounding_set_id: Uuid) -> bool {
        self.affected_routes.iter().any(|route| {
            route.argument_id == argument_id && route.grounding_set_id == grounding_set_id
        })
    }

    pub(super) fn can_restore(&self, route: &super::GroundingRouteRef) -> bool {
        self.reconsiderable_suspensions
            .iter()
            .any(|item| &item.route == route)
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
    let testimony_dates = (
        input.testimony_from.as_deref(),
        input.testimony_through.as_deref(),
    );
    let dates_valid = match testimony_dates {
        (Some(from), Some(through)) => {
            calendar_range(from, through) && through <= input.known_through.as_str()
        }
        (None, None) => true,
        _ => false,
    };
    if !calendar_date(&input.known_through)
        || !dates_valid
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
    let (date_order, order_reports, disposition) = if available {
        let exposure_dates = (
            exposure.input.effective_from.as_deref(),
            exposure.input.effective_through.as_deref(),
        );
        let date_order = match (exposure_dates, testimony_dates) {
            ((Some(_), Some(exposure_through)), (Some(testimony_from), Some(_)))
                if exposure_through < testimony_from =>
            {
                Some(Direction::ExposureBeforeTestimony)
            }
            ((Some(exposure_from), Some(_)), (Some(_), Some(testimony_through)))
                if exposure_from > testimony_through =>
            {
                Some(Direction::ExposureAfterTestimony)
            }
            _ => None,
        };
        let reports = exposure_order::for_review(tx, workspace, situation, input).await?;
        let has_direction = |direction| {
            date_order == Some(direction)
                || reports
                    .active
                    .iter()
                    .any(|report| report.input.direction == direction)
        };
        let disposition = match (
            has_direction(Direction::ExposureBeforeTestimony),
            has_direction(Direction::ExposureAfterTestimony),
        ) {
            (true, true) => Disposition::OrderConflict,
            (true, false) => Disposition::ReviewCandidate,
            (false, true) => Disposition::ExposureAfterTestimony,
            (false, false) => Disposition::OrderUncertain,
        };
        (date_order, reports, disposition)
    } else {
        (
            None,
            exposure_order::ReviewReports::default(),
            Disposition::ExposureNotAvailableAtCutoff,
        )
    };

    let detail_needs_review = disposition == Disposition::ReviewCandidate;
    let needs_finer_timing = disposition == Disposition::OrderUncertain;
    let reconsiderable_suspensions = if disposition == Disposition::OrderConflict
        || (!detail_needs_review && !order_reports.withdrawn.is_empty())
    {
        let comparison = serde_json::json!({
            "witness_actor_id": input.witness_actor_id,
            "exposure_record_id": input.exposure_record_id,
            "testimony_detail": input.testimony_detail,
            "candidate_source": input.candidate_source,
        });
        let relevant_withdrawal =
            (disposition != Disposition::OrderConflict).then_some(input.known_through.as_str());
        route_use::reconsiderable_suspensions(
            tx,
            workspace,
            situation,
            &comparison,
            relevant_withdrawal,
        )
        .await?
    } else {
        Vec::new()
    };
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
        date_order,
        order_reports: order_reports.active,
        order_reports_truncated: order_reports.truncated,
        withdrawn_order_reports: order_reports.withdrawn,
        reconsiderable_suspensions,
        disposition,
        needs_finer_timing,
        detail_needs_review,
        affected_routes,
    })
}
