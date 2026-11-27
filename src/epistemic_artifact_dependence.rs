//! Claim-specific reviews of human-declared derivation between artifact grounds.
use super::{ArtifactDependenceReview, GroundingRouteRef, WorkbenchError};
use sqlx::{Postgres, Transaction};
use std::collections::BTreeMap;
use uuid::Uuid;

type ReviewRow = (
    Uuid,
    Uuid,
    Uuid,
    Uuid,
    Option<Uuid>,
    Option<Uuid>,
    Option<String>,
    Uuid,
    Uuid,
    Uuid,
    bool,
    bool,
);

pub(super) async fn reviews_in(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    relationship: Option<Uuid>,
) -> Result<BTreeMap<Uuid, Vec<ArtifactDependenceReview>>, WorkbenchError> {
    let rows: Vec<ReviewRow> = sqlx::query_as(
        "WITH derivations AS (
           SELECT relationship_id, subject_artifact_id, object_artifact_id,
                  subject_representation_id, object_representation_id,
                  qualification, recorded_by_principal_id
           FROM niccolo.situation_artifact_relationships
           WHERE workspace_id=$1 AND situation_id=$2 AND ($3::uuid IS NULL OR relationship_id=$3)
             AND relationship_type IN ('CLAIMED_TO_BE_DERIVED_FROM','DERIVED_FROM')
         ), cited_artifacts AS (
           SELECT DISTINCT a.conclusion_object_id, a.argument_id, g.grounding_set_id,
                           r.artifact_id, r.representation_id
           FROM niccolo.epistemic_arguments a
           JOIN niccolo.epistemic_grounding_sets g USING(workspace_id,argument_id)
           JOIN niccolo.epistemic_grounding_members m USING(workspace_id,argument_id,grounding_set_id)
           JOIN niccolo.workbench_artifact_provenance p
             ON (p.workspace_id,p.situation_id,p.object_id)=(a.workspace_id,a.situation_id,m.ground_object_id)
           JOIN niccolo.situation_artifact_representations r
             ON (r.workspace_id,r.situation_id,r.representation_id)=(p.workspace_id,p.situation_id,p.representation_id)
           WHERE a.workspace_id=$1 AND a.situation_id=$2 AND g.effect='SUPPORTS' AND m.member_role<>'CONTEXT'
         ), cited_routes AS (
           SELECT c.conclusion_object_id, d.relationship_id, d.subject_artifact_id, d.object_artifact_id,
                  d.subject_representation_id, d.object_representation_id,
                  d.qualification, d.recorded_by_principal_id, c.argument_id, c.grounding_set_id,
                  bool_or(c.artifact_id=d.object_artifact_id) AS cites_source
           FROM derivations d JOIN cited_artifacts c ON
             (c.artifact_id=d.subject_artifact_id AND
               (d.subject_representation_id IS NULL OR c.representation_id=d.subject_representation_id))
             OR (c.artifact_id=d.object_artifact_id AND
               (d.object_representation_id IS NULL OR c.representation_id=d.object_representation_id))
           GROUP BY c.conclusion_object_id, d.relationship_id, d.subject_artifact_id, d.object_artifact_id,
                    d.subject_representation_id, d.object_representation_id,
                    d.qualification, d.recorded_by_principal_id, c.argument_id, c.grounding_set_id
         )
         SELECT c.*, COALESCE(latest.disposition='SUSPEND',false) AS suspended
         FROM cited_routes c
         LEFT JOIN LATERAL (
           SELECT disposition FROM niccolo.epistemic_route_use_decisions h
           WHERE (h.workspace_id,h.situation_id,h.argument_id,h.grounding_set_id) =
                 ($1,$2,c.argument_id,c.grounding_set_id)
           ORDER BY event_no DESC LIMIT 1
         ) latest ON true
         ORDER BY c.conclusion_object_id,c.relationship_id,c.argument_id,c.grounding_set_id",
    )
    .bind(workspace)
    .bind(situation)
    .bind(relationship)
    .fetch_all(&mut **tx)
    .await?;

    let mut candidates = BTreeMap::new();
    for (
        conclusion,
        relationship_id,
        derived_artifact_id,
        source_artifact_id,
        derived_representation_id,
        source_representation_id,
        qualification,
        recorded_by,
        argument_id,
        grounding_set_id,
        cites_source,
        suspended,
    ) in rows
    {
        let review = candidates
            .entry((conclusion, relationship_id))
            .or_insert_with(|| ArtifactDependenceReview {
                relationship_id,
                derived_artifact_id,
                source_artifact_id,
                derived_representation_id,
                source_representation_id,
                qualification,
                recorded_by,
                routes_to_review: Vec::new(),
                suspended_routes: Vec::new(),
                source_routes: Vec::new(),
            });
        let route = GroundingRouteRef {
            argument_id,
            grounding_set_id,
        };
        // Citing both artifacts already exposes the common basis in this route.
        if cites_source {
            review.source_routes.push(route);
        } else if suspended {
            review.suspended_routes.push(route);
        } else {
            review.routes_to_review.push(route);
        }
    }
    let mut by_conclusion = BTreeMap::<Uuid, Vec<ArtifactDependenceReview>>::new();
    for ((conclusion, _), review) in candidates {
        if !review.source_routes.is_empty()
            && (!review.routes_to_review.is_empty() || !review.suspended_routes.is_empty())
        {
            by_conclusion.entry(conclusion).or_default().push(review);
        }
    }
    Ok(by_conclusion)
}
