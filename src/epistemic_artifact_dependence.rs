//! Claim-specific reviews of human-declared derivation between artifact grounds.
use super::{
    ArtifactDependenceReview, ArtifactDependenceVia, GroundingRouteRef, SharedArtifactOrigin,
};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use uuid::Uuid;

type ReviewRow = (
    Uuid,
    Uuid,
    Option<Uuid>,
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
    Option<String>,
    Option<String>,
    Option<String>,
);

#[derive(Default)]
pub(crate) struct ArtifactDependenceReport {
    pub(crate) reviews: BTreeMap<Uuid, Vec<ArtifactDependenceReview>>,
    pub(crate) shared_origins: BTreeMap<Uuid, Vec<SharedArtifactOrigin>>,
}

pub(super) async fn reviews_in(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    relationship: Option<Uuid>,
    via_relationship: Option<Uuid>,
) -> Result<ArtifactDependenceReport, sqlx::Error> {
    load(
        tx,
        workspace,
        situation,
        relationship,
        via_relationship,
        None,
        None,
    )
    .await
}

pub(crate) async fn frozen_origins_in(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    revision: i64,
    routes: &Value,
) -> Result<BTreeMap<Uuid, Vec<SharedArtifactOrigin>>, sqlx::Error> {
    Ok(load(
        tx,
        workspace,
        situation,
        None,
        None,
        Some(revision),
        Some(routes),
    )
    .await?
    .shared_origins)
}

async fn load(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    relationship: Option<Uuid>,
    via_relationship: Option<Uuid>,
    revision: Option<i64>,
    frozen_routes: Option<&Value>,
) -> Result<ArtifactDependenceReport, sqlx::Error> {
    let rows: Vec<ReviewRow> = sqlx::query_as(
        "WITH derivations AS (
           SELECT relationship_id, subject_artifact_id, object_artifact_id,
                  subject_representation_id, object_representation_id,
                  qualification, recorded_by_principal_id
           FROM niccolo.situation_artifact_relationships d
           WHERE workspace_id=$1 AND situation_id=$2
             AND relationship_type IN ('CLAIMED_TO_BE_DERIVED_FROM','DERIVED_FROM')
             AND ($5::bigint IS NULL OR EXISTS (
               SELECT 1 FROM niccolo.situation_revision_artifact_relationships rr
               WHERE (rr.workspace_id,rr.situation_id,rr.revision,rr.relationship_id)=
                     (d.workspace_id,d.situation_id,$5,d.relationship_id)
             ))
         ), dependence AS (
           SELECT d.*, NULL::uuid AS via_relationship_id FROM derivations d
           WHERE ($3::uuid IS NULL OR d.relationship_id=$3) AND $4::uuid IS NULL
           UNION ALL
           SELECT first.relationship_id, first.subject_artifact_id, second.object_artifact_id,
                  first.subject_representation_id, second.object_representation_id,
                  first.qualification, first.recorded_by_principal_id, second.relationship_id
           FROM derivations first JOIN derivations second ON
             (first.object_artifact_id,first.object_representation_id) =
             (second.subject_artifact_id,second.subject_representation_id)
           WHERE first.subject_artifact_id<>second.object_artifact_id
             AND ($3::uuid IS NULL OR (first.relationship_id=$3 AND second.relationship_id=$4))
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
           WHERE $5::bigint IS NULL AND a.workspace_id=$1 AND a.situation_id=$2
             AND g.effect='SUPPORTS' AND m.member_role<>'CONTEXT'
           UNION
           SELECT route.conclusion_object_id,route.argument_id,route.grounding_set_id,r.artifact_id,r.representation_id
           FROM jsonb_to_recordset($6::text::jsonb) AS route(
             conclusion_object_id uuid,argument_id uuid,grounding_set_id uuid,grounds jsonb)
           CROSS JOIN LATERAL jsonb_array_elements(route.grounds) AS ground(value)
           CROSS JOIN LATERAL jsonb_array_elements_text(ground.value->'artifact_representation_ids') AS representation(id)
           JOIN niccolo.situation_artifact_representations r
             ON (r.workspace_id,r.situation_id,r.representation_id)=($1,$2,representation.id::uuid)
         ), cited_routes AS (
           SELECT c.conclusion_object_id, d.relationship_id, d.via_relationship_id,
                  d.subject_artifact_id, d.object_artifact_id,
                  d.subject_representation_id, d.object_representation_id,
                  d.qualification, d.recorded_by_principal_id, c.argument_id, c.grounding_set_id,
                  bool_or(c.artifact_id=d.object_artifact_id) AS cites_source
           FROM dependence d JOIN cited_artifacts c ON
             (c.artifact_id=d.subject_artifact_id AND
               (d.subject_representation_id IS NULL OR c.representation_id=d.subject_representation_id))
             OR (c.artifact_id=d.object_artifact_id AND
               (d.object_representation_id IS NULL OR c.representation_id=d.object_representation_id))
           GROUP BY c.conclusion_object_id, d.relationship_id, d.via_relationship_id,
                    d.subject_artifact_id, d.object_artifact_id,
                    d.subject_representation_id, d.object_representation_id,
                    d.qualification, d.recorded_by_principal_id, c.argument_id, c.grounding_set_id
         )
         SELECT c.*, COALESCE(latest.disposition='SUSPEND',false) AS suspended,
                CASE WHEN middle.relationship_id IS NOT NULL THEN jsonb_build_object(
                  'relationship_id',middle.relationship_id,'artifact_id',middle.subject_artifact_id,
                  'representation_id',middle.subject_representation_id,'artifact_label',a.label,
                  'scope_locator',r.scope_locator,'qualification',middle.qualification,
                  'recorded_by',middle.recorded_by_principal_id
                )::text END AS via,
                origin_artifact.label, origin_passage.scope_locator
         FROM cited_routes c
         LEFT JOIN derivations middle ON middle.relationship_id=c.via_relationship_id
         LEFT JOIN niccolo.situation_artifact_representations r
           ON (r.workspace_id,r.situation_id,r.representation_id)=($1,$2,middle.subject_representation_id)
         LEFT JOIN niccolo.artifacts a ON (a.workspace_id,a.artifact_id)=($1,middle.subject_artifact_id)
         LEFT JOIN niccolo.situation_artifact_representations origin_passage
           ON (origin_passage.workspace_id,origin_passage.situation_id,origin_passage.representation_id)=
              ($1,$2,c.object_representation_id)
         LEFT JOIN niccolo.artifacts origin_artifact
           ON (origin_artifact.workspace_id,origin_artifact.artifact_id)=($1,origin_passage.artifact_id)
         LEFT JOIN LATERAL (
           SELECT disposition FROM niccolo.epistemic_route_use_decisions h
           WHERE (h.workspace_id,h.situation_id,h.argument_id,h.grounding_set_id) =
                 ($1,$2,c.argument_id,c.grounding_set_id)
           ORDER BY event_no DESC LIMIT 1
         ) latest ON $5::bigint IS NULL
         ORDER BY c.conclusion_object_id,c.relationship_id,c.via_relationship_id,c.argument_id,c.grounding_set_id",
    )
    .bind(workspace)
    .bind(situation)
    .bind(relationship)
    .bind(via_relationship)
    .bind(revision)
    .bind(frozen_routes.map(Value::to_string))
    .fetch_all(&mut **tx)
    .await?;

    let mut candidates = BTreeMap::new();
    for (
        conclusion,
        relationship_id,
        via_relationship_id,
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
        via,
        source_artifact_label,
        source_scope_locator,
    ) in rows
    {
        let (review, _) = match candidates.entry((conclusion, relationship_id, via_relationship_id))
        {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let via = via
                    .as_deref()
                    .map(serde_json::from_str::<ArtifactDependenceVia>)
                    .transpose()
                    .map_err(|error| sqlx::Error::Decode(error.into()))?;
                entry.insert((
                    ArtifactDependenceReview {
                        relationship_id,
                        via,
                        derived_artifact_id,
                        source_artifact_id,
                        derived_representation_id,
                        source_representation_id,
                        qualification,
                        recorded_by,
                        routes_to_review: Vec::new(),
                        suspended_routes: Vec::new(),
                        source_routes: Vec::new(),
                    },
                    source_artifact_label.zip(source_scope_locator),
                ))
            }
        };
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
    let mut report = ArtifactDependenceReport::default();
    let mut shared_origins = BTreeMap::new();
    for ((conclusion, _, _), (review, labels)) in candidates {
        if !review.source_routes.is_empty()
            && (!review.routes_to_review.is_empty() || !review.suspended_routes.is_empty())
        {
            report.reviews.entry(conclusion).or_default().push(review);
        } else if relationship.is_none()
            && !review.routes_to_review.is_empty()
            && let Some(source_representation_id) = review.source_representation_id
            && let Some((source_artifact_label, source_scope_locator)) = labels
        {
            let origin = shared_origins
                .entry((conclusion, source_representation_id))
                .or_insert_with(|| SharedArtifactOrigin {
                    source_artifact_id: review.source_artifact_id,
                    source_representation_id,
                    source_artifact_label,
                    source_scope_locator,
                    derivations: Vec::new(),
                });
            origin.derivations.push(review);
        }
    }
    for ((conclusion, _), origin) in shared_origins {
        let copies = origin
            .derivations
            .iter()
            .map(|copy| copy.derived_artifact_id)
            .collect::<BTreeSet<_>>();
        let routes = origin
            .derivations
            .iter()
            .flat_map(|copy| &copy.routes_to_review)
            .collect::<BTreeSet<_>>();
        if copies.len() > 1 && routes.len() > 1 {
            report
                .shared_origins
                .entry(conclusion)
                .or_default()
                .push(origin);
        }
    }
    Ok(report)
}
