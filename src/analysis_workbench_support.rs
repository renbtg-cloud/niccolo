//! One analysis's recorded view of authored Workbench support routes.
use crate::analysis_structure::AnalysisScope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Basis {
    pub routes: Vec<Route>,
    pub truncated: bool,
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

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupportLink {
    pub claim_id: Uuid,
    pub argument_id: Uuid,
    pub grounding_set_id: Uuid,
}

impl Basis {
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
) -> Result<Option<String>, sqlx::Error> {
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
               'text',left(t.text,384),'rationale',left(m.rationale,256)
             ) AS ground
             FROM niccolo.epistemic_grounding_members m LEFT JOIN texts t ON t.object_id=m.ground_object_id
             WHERE m.workspace_id=$1 AND m.argument_id=r.argument_id AND m.grounding_set_id=r.grounding_set_id
               AND m.member_role<>'CONTEXT'
             ORDER BY m.member_id LIMIT 17
           ) ranked
         ) m ON r.suspended_by_decision_id IS NULL
         ORDER BY r.created_at,r.argument_id,r.grounding_set_id",
    )
    .bind(workspace)
    .bind(situation)
    .fetch_all(&mut **tx)
    .await?;
    if rows.is_empty() {
        return Ok(None);
    }
    let truncated = rows.len() > 32;
    let routes = rows
        .iter()
        .take(32)
        .map(|row| serde_json::from_str(row))
        .collect::<Result<Vec<Route>, _>>()
        .map_err(|error| sqlx::Error::Decode(error.into()))?;
    serde_json::to_string(&Basis { routes, truncated })
        .map(Some)
        .map_err(|error| sqlx::Error::Encode(error.into()))
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
