//! Human decisions about whether to use an authored support route.
use super::{
    GroundingRouteRef, WorkbenchError, artifact_dependence, detail_exposure, scoped, valid_text,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Disposition {
    Use,
    Suspend,
}

impl Disposition {
    fn db_value(self) -> &'static str {
        match self {
            Self::Use => "USE",
            Self::Suspend => "SUSPEND",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub argument_id: Uuid,
    pub grounding_set_id: Uuid,
    pub disposition: Disposition,
    pub rationale: String,
    pub review: detail_exposure::Input,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactDependenceInput {
    pub argument_id: Uuid,
    pub grounding_set_id: Uuid,
    pub disposition: Disposition,
    pub rationale: String,
    pub relationship_id: Uuid,
    pub via_relationship_id: Option<Uuid>,
}

enum ReviewBasis {
    Exposure(detail_exposure::Input),
    ArtifactDependence {
        relationship_id: Uuid,
        via_relationship_id: Option<Uuid>,
    },
}

struct Choice {
    route: GroundingRouteRef,
    disposition: Disposition,
    rationale: String,
    basis: ReviewBasis,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Decision {
    pub event_id: Uuid,
    pub route: GroundingRouteRef,
    pub disposition: Disposition,
    pub rationale: String,
    pub recorded_by: Uuid,
    pub recorded_at: String,
    pub basis: Value,
}

type StoredRouteDecisionRow = (Uuid, Uuid, Uuid, String, String, Uuid, String, String);

async fn load(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    event_id: Option<Uuid>,
) -> Result<Vec<Decision>, WorkbenchError> {
    let rows: Vec<StoredRouteDecisionRow> = sqlx::query_as(
        "SELECT event_id, argument_id, grounding_set_id, disposition, rationale, recorded_by,
                recorded_at::text, basis::text
         FROM niccolo.epistemic_route_use_decisions
         WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR event_id=$3)
         ORDER BY event_no",
    )
    .bind(workspace)
    .bind(situation)
    .bind(event_id)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(
            |(
                event_id,
                argument_id,
                grounding_set_id,
                disposition,
                rationale,
                recorded_by,
                recorded_at,
                basis,
            )| {
                Ok(Decision {
                    event_id,
                    route: GroundingRouteRef {
                        argument_id,
                        grounding_set_id,
                    },
                    disposition: match disposition.as_str() {
                        "USE" => Disposition::Use,
                        "SUSPEND" => Disposition::Suspend,
                        _ => return Err(WorkbenchError::Invalid),
                    },
                    rationale,
                    recorded_by,
                    recorded_at,
                    basis: serde_json::from_str(&basis).map_err(|_| WorkbenchError::Invalid)?,
                })
            },
        )
        .collect()
}

pub(super) async fn history(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
) -> Result<Vec<Decision>, WorkbenchError> {
    load(tx, workspace, situation, None).await
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    event_id: Uuid,
    author: Uuid,
    input: Input,
) -> Result<(Decision, bool), WorkbenchError> {
    record_choice(
        pool,
        workspace,
        situation,
        event_id,
        author,
        Choice {
            route: GroundingRouteRef {
                argument_id: input.argument_id,
                grounding_set_id: input.grounding_set_id,
            },
            disposition: input.disposition,
            rationale: input.rationale,
            basis: ReviewBasis::Exposure(input.review),
        },
    )
    .await
}

pub async fn record_artifact_dependence(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    event_id: Uuid,
    author: Uuid,
    input: ArtifactDependenceInput,
) -> Result<(Decision, bool), WorkbenchError> {
    record_choice(
        pool,
        workspace,
        situation,
        event_id,
        author,
        Choice {
            route: GroundingRouteRef {
                argument_id: input.argument_id,
                grounding_set_id: input.grounding_set_id,
            },
            disposition: input.disposition,
            rationale: input.rationale,
            basis: ReviewBasis::ArtifactDependence {
                relationship_id: input.relationship_id,
                via_relationship_id: input.via_relationship_id,
            },
        },
    )
    .await
}

async fn record_choice(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    event_id: Uuid,
    author: Uuid,
    choice: Choice,
) -> Result<(Decision, bool), WorkbenchError> {
    if event_id.is_nil() || !valid_text(&choice.rationale, 4096) {
        return Err(WorkbenchError::Invalid);
    }
    let rationale = choice.rationale.trim();
    let review_input = match &choice.basis {
        ReviewBasis::Exposure(input) => {
            serde_json::to_value(input).map_err(|_| WorkbenchError::Invalid)?
        }
        ReviewBasis::ArtifactDependence {
            relationship_id,
            via_relationship_id,
        } => {
            let mut identity = json!({"relationship_id":relationship_id});
            if let Some(via) = via_relationship_id {
                identity["via_relationship_id"] = json!(via);
            }
            identity
        }
    };
    let mut tx = scoped(pool, workspace, false).await?;
    let mut stored = load(&mut tx, workspace, situation, Some(event_id))
        .await?
        .pop();
    let created = if stored.is_none() {
        let result = match &choice.basis {
            ReviewBasis::Exposure(input) => {
                let result =
                    detail_exposure::review_in(&mut tx, workspace, situation, input).await?;
                if !result.flags_route(choice.route.argument_id, choice.route.grounding_set_id) {
                    return Err(WorkbenchError::Invalid);
                }
                json!(result)
            }
            ReviewBasis::ArtifactDependence {
                relationship_id,
                via_relationship_id,
            } => {
                let reviews = artifact_dependence::reviews_in(
                    &mut tx,
                    workspace,
                    situation,
                    Some(*relationship_id),
                    *via_relationship_id,
                )
                .await?;
                let (conclusion, review) = reviews
                    .reviews
                    .into_iter()
                    .flat_map(|(conclusion, reviews)| {
                        reviews.into_iter().map(move |review| (conclusion, review))
                    })
                    .find(|(_, review)| {
                        review.routes_to_review.contains(&choice.route)
                            || review.suspended_routes.contains(&choice.route)
                    })
                    .ok_or(WorkbenchError::Invalid)?;
                json!({"conclusion_object_id":conclusion, "review":review})
            }
        };
        let basis = json!({"input": review_input, "result": result}).to_string();
        let created = sqlx::query(
            "INSERT INTO niccolo.epistemic_route_use_decisions
               (workspace_id,situation_id,event_id,argument_id,grounding_set_id,
                disposition,rationale,basis,recorded_by)
             SELECT $1,$2,$3,$4,$5,$6,$7,$8::text::jsonb,$9
             WHERE EXISTS (
                SELECT 1 FROM niccolo.epistemic_grounding_sets g
                JOIN niccolo.epistemic_arguments a USING(workspace_id,argument_id)
                WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.argument_id=$4
                  AND g.grounding_set_id=$5 AND g.effect='SUPPORTS'
             )
             ON CONFLICT (workspace_id,situation_id,event_id) DO NOTHING",
        )
        .bind(workspace)
        .bind(situation)
        .bind(event_id)
        .bind(choice.route.argument_id)
        .bind(choice.route.grounding_set_id)
        .bind(choice.disposition.db_value())
        .bind(rationale)
        .bind(basis)
        .bind(author)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        stored = load(&mut tx, workspace, situation, Some(event_id))
            .await?
            .pop();
        created
    } else {
        false
    };
    let decision = stored.ok_or(WorkbenchError::Invisible)?;
    if decision.route != choice.route
        || decision.disposition != choice.disposition
        || decision.rationale != rationale
        || decision.recorded_by != author
        || decision.basis["input"] != review_input
    {
        return Err(WorkbenchError::Conflict);
    }
    tx.commit().await?;
    Ok((decision, created))
}
