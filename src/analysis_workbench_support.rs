//! One analysis's recorded view of authored Workbench support routes.
use crate::analysis_structure::AnalysisScope;
use crate::epistemic_workbench::{
    ArtifactDependenceReview, GroundingRouteRef, artifact_dependence,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Basis {
    pub routes: Vec<Route>,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_artifact_origins: Vec<FrozenSharedOrigin>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shared_artifact_origins_truncated: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FrozenSharedOrigin {
    pub conclusion_object_id: Uuid,
    pub source_artifact_id: Uuid,
    pub source_representation_id: Uuid,
    pub source_artifact_label: String,
    pub source_scope_locator: String,
    pub derivations: Vec<FrozenOriginDerivation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FrozenOriginDerivation {
    pub relationship_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via_relationship_id: Option<Uuid>,
    pub routes: Vec<GroundingRouteRef>,
}

const MAX_ORIGIN_DERIVATIONS: usize = 16;

fn supporting_pair(derivations: &[ArtifactDependenceReview]) -> Option<(usize, usize)> {
    let first = derivations.first()?;
    let first_route = first.routes_to_review.first()?;
    let other_artifact =
        |review: &ArtifactDependenceReview| review.derived_artifact_id != first.derived_artifact_id;
    let other_route = |review: &ArtifactDependenceReview| {
        review
            .routes_to_review
            .iter()
            .any(|route| route != first_route)
    };
    if let Some(second) = derivations
        .iter()
        .position(|review| other_artifact(review) && other_route(review))
    {
        return Some((0, second));
    }
    Some((
        derivations.iter().position(other_route)?,
        derivations.iter().position(other_artifact)?,
    ))
}

fn bounded_derivations(derivations: Vec<ArtifactDependenceReview>) -> Vec<FrozenOriginDerivation> {
    let mut keep = BTreeSet::new();
    if derivations.len() > MAX_ORIGIN_DERIVATIONS
        && let Some((first, second)) = supporting_pair(&derivations)
    {
        keep.extend([first, second]);
    }
    for index in 0..derivations.len() {
        if keep.len() == MAX_ORIGIN_DERIVATIONS {
            break;
        }
        keep.insert(index);
    }
    derivations
        .into_iter()
        .enumerate()
        .filter(|(index, _)| keep.contains(index))
        .map(|(_, derivation)| FrozenOriginDerivation {
            relationship_id: derivation.relationship_id,
            via_relationship_id: derivation.via.map(|via| via.relationship_id),
            routes: derivation.routes_to_review,
        })
        .collect()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Route {
    pub argument_id: Uuid,
    pub grounding_set_id: Uuid,
    pub conclusion_object_id: Uuid,
    pub conclusion_text: Option<String>,
    pub adequacy: String,
    pub grounds: Vec<Value>,
    pub grounds_truncated: bool,
    pub suspended_by_decision_id: Option<Uuid>,
    pub suspension_rationale: Option<String>,
}

impl Route {
    fn provenance_was_limited(&self) -> bool {
        self.suspended_by_decision_id.is_none()
            && (self.grounds_truncated
                || self
                    .grounds
                    .iter()
                    .any(|ground| ground["artifact_provenance_truncated"] == Value::Bool(true)))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupportLink {
    pub claim_id: Uuid,
    pub argument_id: Uuid,
    pub grounding_set_id: Uuid,
}

impl Basis {
    pub(crate) fn shared_origins_are_partial(&self) -> bool {
        self.truncated
            || self.shared_artifact_origins_truncated
            || self.routes.iter().any(Route::provenance_was_limited)
    }

    pub(crate) fn allows(&self, link: &SupportLink) -> bool {
        self.routes.iter().any(|route| {
            route.argument_id == link.argument_id
                && route.grounding_set_id == link.grounding_set_id
                && route.suspended_by_decision_id.is_none()
                && !route.grounds_truncated
        })
    }
}

pub(crate) async fn capture(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    input_revision: i64,
) -> Result<Option<String>, sqlx::Error> {
    capture_basis(tx, workspace, situation, input_revision)
        .await?
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| sqlx::Error::Encode(error.into()))
}

pub(crate) async fn capture_basis(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    input_revision: i64,
) -> Result<Option<Basis>, sqlx::Error> {
    // All decisions and grounds in this projection come from one statement snapshot.
    let rows: Vec<String> = sqlx::query_scalar(
        "WITH routes AS (
           SELECT a.argument_id, g.grounding_set_id, a.conclusion_object_id,
                  g.adequacy, g.created_at,
                  CASE WHEN d.disposition='SUSPEND' THEN d.event_id END AS suspended_by_decision_id,
                  CASE WHEN d.disposition='SUSPEND' THEN left(d.rationale,512) END AS suspension_rationale
           FROM niccolo.epistemic_arguments a JOIN niccolo.epistemic_grounding_sets g
             USING(workspace_id,argument_id)
           LEFT JOIN LATERAL (
             SELECT event_id, disposition, rationale FROM niccolo.epistemic_route_use_decisions d
             WHERE (d.workspace_id,d.situation_id,d.argument_id,d.grounding_set_id) =
               (a.workspace_id,a.situation_id,a.argument_id,g.grounding_set_id)
             ORDER BY event_no DESC LIMIT 1
           ) d ON true
           WHERE a.workspace_id=$1 AND a.situation_id=$2 AND g.effect='SUPPORTS'
             AND EXISTS (SELECT 1 FROM niccolo.epistemic_grounding_members m
               WHERE (m.workspace_id,m.argument_id,m.grounding_set_id) =
                 (a.workspace_id,a.argument_id,g.grounding_set_id) AND m.member_role<>'CONTEXT')
           ORDER BY g.created_at,a.argument_id,g.grounding_set_id LIMIT 33
         ), texts AS (
           SELECT p.proposition_id AS object_id,p.statement_text AS text
             FROM niccolo.epistemic_situation_propositions sp JOIN niccolo.epistemic_propositions p
               USING(workspace_id,proposition_id)
             WHERE sp.workspace_id=$1 AND sp.situation_id=$2
           UNION ALL SELECT hypothesis_id,statement_text FROM niccolo.epistemic_hypotheses
             WHERE workspace_id=$1 AND situation_id=$2
           UNION ALL SELECT assumption_id,statement_text FROM niccolo.epistemic_assumptions
             WHERE workspace_id=$1 AND situation_id=$2
           UNION ALL SELECT finding_id,statement_text FROM niccolo.epistemic_findings
             WHERE workspace_id=$1 AND situation_id=$2
         )
         SELECT jsonb_build_object(
           'argument_id',r.argument_id,'grounding_set_id',r.grounding_set_id,
           'conclusion_object_id',r.conclusion_object_id,'conclusion_text',left(c.text,384),
           'adequacy',r.adequacy,'grounds',COALESCE(m.grounds,'[]'::jsonb),
           'grounds_truncated',COALESCE(m.truncated,false),
           'suspended_by_decision_id',r.suspended_by_decision_id,'suspension_rationale',r.suspension_rationale
         )::text
         FROM routes r LEFT JOIN texts c ON c.object_id=r.conclusion_object_id
         LEFT JOIN LATERAL (
           SELECT jsonb_agg(ground ORDER BY n) FILTER (WHERE n<=16) AS grounds, count(*)>16 AS truncated
           FROM (
             SELECT row_number() OVER (ORDER BY m.member_id) AS n, jsonb_build_object(
               'object_id',m.ground_object_id,'evidence_id',m.evidence_id,'role',m.member_role,
               'text',left(t.text,384),'rationale',left(m.rationale,256),
               'artifact_representation_ids',COALESCE(p.representation_ids,'[]'::jsonb),
               'artifact_provenance_truncated',COALESCE(p.truncated,false)
             ) AS ground
             FROM niccolo.epistemic_grounding_members m LEFT JOIN texts t ON t.object_id=m.ground_object_id
             LEFT JOIN LATERAL (
               SELECT jsonb_agg(representation_id ORDER BY n) FILTER (WHERE n<=16) AS representation_ids,
                      count(*)>16 AS truncated
               FROM (
                 SELECT p.representation_id,row_number() OVER (ORDER BY p.representation_id) AS n
                 FROM niccolo.workbench_artifact_provenance p
                 JOIN niccolo.situation_revision_artifact_representations rr
                   USING(workspace_id,situation_id,representation_id)
                 WHERE p.workspace_id=$1 AND p.situation_id=$2 AND p.object_id=m.ground_object_id
                   AND rr.revision=$3
                 ORDER BY p.representation_id LIMIT 17
               ) linked
             ) p ON true
             WHERE m.workspace_id=$1 AND m.argument_id=r.argument_id AND m.grounding_set_id=r.grounding_set_id
               AND m.member_role<>'CONTEXT'
             ORDER BY m.member_id LIMIT 17
           ) ranked
         ) m ON r.suspended_by_decision_id IS NULL
         ORDER BY r.created_at,r.argument_id,r.grounding_set_id",
    )
    .bind(workspace)
    .bind(situation)
    .bind(input_revision)
    .fetch_all(&mut **tx)
    .await?;
    if rows.is_empty() {
        return Ok(None);
    }
    let truncated = rows.len() > 32;
    let routes: Vec<Route> = rows
        .iter()
        .take(32)
        .map(|row| serde_json::from_str(row))
        .collect::<Result<Vec<Route>, _>>()
        .map_err(|error| sqlx::Error::Decode(error.into()))?;
    let mut basis = Basis {
        routes,
        truncated,
        shared_artifact_origins: Vec::new(),
        shared_artifact_origins_truncated: false,
    };
    basis.shared_artifact_origins_truncated = basis.shared_origins_are_partial();
    let complete_routes = basis
        .routes
        .iter()
        .filter(|route| route.suspended_by_decision_id.is_none() && !route.provenance_was_limited())
        .collect::<Vec<_>>();
    if complete_routes.iter().any(|route| {
        route.grounds.iter().any(|ground| {
            ground["artifact_representation_ids"]
                .as_array()
                .is_some_and(|ids| !ids.is_empty())
        })
    }) {
        let frozen = serde_json::to_value(complete_routes)
            .map_err(|error| sqlx::Error::Encode(error.into()))?;
        let origins = artifact_dependence::frozen_origins_in(
            tx,
            workspace,
            situation,
            input_revision,
            &frozen,
        )
        .await?;
        let mut origins = origins.into_iter().flat_map(|(conclusion, origins)| {
            origins.into_iter().map(move |origin| (conclusion, origin))
        });
        for (conclusion_object_id, origin) in origins.by_ref().take(32) {
            basis.shared_artifact_origins_truncated |=
                origin.derivations.len() > MAX_ORIGIN_DERIVATIONS;
            basis.shared_artifact_origins.push(FrozenSharedOrigin {
                conclusion_object_id,
                source_artifact_id: origin.source_artifact_id,
                source_representation_id: origin.source_representation_id,
                source_artifact_label: origin.source_artifact_label,
                source_scope_locator: origin.source_scope_locator,
                derivations: bounded_derivations(origin.derivations),
            });
        }
        basis.shared_artifact_origins_truncated |= origins.next().is_some();
    }
    Ok(Some(basis))
}

pub(crate) async fn read_in(
    tx: &mut Transaction<'_, Postgres>,
    scope: AnalysisScope,
) -> Result<Option<Basis>, sqlx::Error> {
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT workbench_support_basis::text FROM niccolo.situation_analysis_snapshots
         WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3",
    )
    .bind(scope.workspace)
    .bind(scope.situation)
    .bind(scope.analysis)
    .fetch_optional(&mut **tx)
    .await?
    .flatten();
    stored
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|error| sqlx::Error::Decode(error.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn derivation(id: u128, artifact: u128, route: u128) -> ArtifactDependenceReview {
        ArtifactDependenceReview {
            relationship_id: Uuid::from_u128(id),
            via: None,
            derived_artifact_id: Uuid::from_u128(artifact),
            source_artifact_id: Uuid::from_u128(90),
            derived_representation_id: None,
            source_representation_id: Some(Uuid::from_u128(91)),
            qualification: None,
            recorded_by: Uuid::from_u128(92),
            routes_to_review: vec![GroundingRouteRef {
                argument_id: Uuid::from_u128(93),
                grounding_set_id: Uuid::from_u128(route),
            }],
            suspended_routes: Vec::new(),
            source_routes: Vec::new(),
        }
    }

    #[test]
    fn derivation_budget_keeps_a_pair_with_distinct_copies_and_routes() {
        let mut derivations = (1..=20).map(|id| derivation(id, 1, 5)).collect::<Vec<_>>();
        derivations.push(derivation(21, 1, 6));
        derivations.push(derivation(22, 2, 5));
        let frozen = bounded_derivations(derivations);
        assert_eq!(frozen.len(), MAX_ORIGIN_DERIVATIONS);
        assert_eq!(frozen[14].relationship_id, Uuid::from_u128(21));
        assert_eq!(frozen[15].relationship_id, Uuid::from_u128(22));
        assert_ne!(frozen[14].routes, frozen[15].routes);
    }

    #[test]
    fn older_bases_still_report_partial_origins_when_routes_or_provenance_were_omitted() {
        let route = Route {
            argument_id: Uuid::from_u128(1),
            grounding_set_id: Uuid::from_u128(2),
            conclusion_object_id: Uuid::from_u128(3),
            conclusion_text: None,
            adequacy: "RELEVANT".into(),
            grounds: vec![serde_json::json!({"artifact_provenance_truncated": true})],
            grounds_truncated: false,
            suspended_by_decision_id: None,
            suspension_rationale: None,
        };
        let mut basis = Basis {
            routes: vec![route],
            truncated: false,
            shared_artifact_origins: Vec::new(),
            shared_artifact_origins_truncated: false,
        };
        assert!(basis.shared_origins_are_partial());
        basis.routes[0].grounds.clear();
        basis.routes[0].grounds_truncated = true;
        assert!(basis.shared_origins_are_partial());
        basis.routes[0].suspended_by_decision_id = Some(Uuid::from_u128(4));
        assert!(!basis.shared_origins_are_partial());
        basis.truncated = true;
        assert!(basis.shared_origins_are_partial());
    }
}
