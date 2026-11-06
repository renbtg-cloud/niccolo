use niccolo_evidence::action_authorization::{self, AuthorizationEvent};
use niccolo_evidence::analysis_actions::{self, ActionDecision, ActionProposal};
use niccolo_evidence::analysis_claims::{self, NewClaim};
use niccolo_evidence::analysis_structure::{self, AnalysisScope, Uncertainty};
use niccolo_evidence::analysis_timeline;
use niccolo_evidence::attention_control::{self, PinEvent};
use niccolo_evidence::reanalysis_control::{
    self, ReanalysisRequest, ReanalysisSuggestion, SuggestionDecision,
};
use niccolo_evidence::review_session::{self, ReviewCheckpoint};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Seed {
    workspace: Uuid,
    situation: Uuid,
    principal: Uuid,
    analysis1: Uuid,
    analysis2: Uuid,
}

async fn pools() -> (PgPool, PgPool) {
    let admin_url = std::env::var("NICCOLO_PG_ADMIN_URL")
        .unwrap_or_else(|_| "postgres://oai@127.0.0.1:55432/niccolo_test".to_string());
    let app_url = std::env::var("NICCOLO_PG_SERVICE_URL").unwrap_or_else(|_| {
        "postgres://niccolo_service_test@127.0.0.1:55432/niccolo_test".to_string()
    });
    let admin = PgPoolOptions::new()
        .max_connections(8)
        .connect(&admin_url)
        .await
        .unwrap();
    let app = PgPoolOptions::new()
        .max_connections(32)
        .connect(&app_url)
        .await
        .unwrap();
    (admin, app)
}

async fn seed(admin: &PgPool) -> Seed {
    let s = Seed {
        workspace: Uuid::new_v4(),
        situation: Uuid::new_v4(),
        principal: Uuid::new_v4(),
        analysis1: Uuid::new_v4(),
        analysis2: Uuid::new_v4(),
    };
    sqlx::query("INSERT INTO niccolo_auth.principals(principal_id,display_name) VALUES($1,'user')")
        .bind(s.principal)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,name) VALUES($1,'w')")
        .bind(s.workspace)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'current focus',$3,2)")
        .bind(s.workspace).bind(s.situation).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id) VALUES($1,$2,1,'old focus',$3,$5),($1,$2,2,'current focus',$4,$5)")
        .bind(s.workspace).bind(s.situation).bind("a".repeat(64)).bind("b".repeat(64)).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id,reanalysis_of) VALUES($1,$2,$3,1,$5,'understand',$6,NULL),($1,$2,$4,2,$7,'reanalyze',$6,$3)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis1).bind(s.analysis2).bind("a".repeat(64)).bind(s.principal).bind("b".repeat(64)).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_outputs(workspace_id,situation_id,analysis_id,answer_text,coverage_note,published_by_principal_id) VALUES($1,$2,$3,'one','partial',$5),($1,$2,$4,'two','partial',$5)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis1).bind(s.analysis2).bind(s.principal).execute(admin).await.unwrap();
    s
}

fn scope(s: Seed) -> AnalysisScope {
    AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis1,
    }
}

async fn make_claim(app: &PgPool, s: Seed, text: &str) -> Uuid {
    let id = Uuid::new_v4();
    analysis_claims::record_claim(
        app,
        s.workspace,
        s.situation,
        s.analysis1,
        s.principal,
        NewClaim {
            claim_id: id,
            ordinal: 1,
            kind: "INFERENCE".into(),
            text: text.into(),
            qualification: Some("provisional".into()),
        },
    )
    .await
    .unwrap();
    id
}

async fn make_uncertainty(app: &PgPool, s: Seed, claim: Option<Uuid>) -> Uuid {
    let id = Uuid::new_v4();
    analysis_structure::record_uncertainty(
        app,
        scope(s),
        &Uncertainty {
            uncertainty_id: id,
            claim_id: claim,
            kind: "UNKNOWN".into(),
            statement: "we do not know yet".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    id
}

async fn make_proposal(app: &PgPool, s: Seed, claim: Option<Uuid>) -> Uuid {
    let id = Uuid::new_v4();
    analysis_actions::record_proposal(
        app,
        scope(s),
        &ActionProposal {
            proposal_id: id,
            claim_id: claim,
            action_text: "ask the human before doing anything".into(),
            rationale: "human owns the project".into(),
            risk_note: Some("no execution implied".into()),
            proposed_by: s.principal,
        },
    )
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn v057_timeline_keyset_pagination_has_no_duplicates() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let claim = make_claim(&app, s, "claim").await;
    analysis_claims::append_review(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        claim,
        Uuid::new_v4(),
        "COMMENT",
        "human note",
        s.principal,
    )
    .await
    .unwrap();
    make_uncertainty(&app, s, Some(claim)).await;
    make_proposal(&app, s, Some(claim)).await;
    let p1 = analysis_timeline::page(&app, scope(s), None, 2)
        .await
        .unwrap();
    assert_eq!(p1.items.len(), 2);
    assert!(p1.next.is_some());
    let p2 = analysis_timeline::page(&app, scope(s), p1.next.as_ref(), 2)
        .await
        .unwrap();
    assert!(!p2.items.is_empty());
    for a in &p1.items {
        assert!(
            !p2.items
                .iter()
                .any(|b| b.object_id == a.object_id && b.event_type == a.event_type)
        );
    }
}

#[tokio::test]
async fn v058_attention_reasons_are_explainable_but_never_auto_ranked() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let claim = make_claim(&app, s, "claim").await;
    make_uncertainty(&app, s, Some(claim)).await;
    make_proposal(&app, s, Some(claim)).await;
    attention_control::append_pin_event(
        &app,
        scope(s),
        &PinEvent {
            event_id: Uuid::new_v4(),
            target_type: "CLAIM".into(),
            target_id: claim,
            kind: "PINNED".into(),
            reason: "I want to inspect this".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let reasons = attention_control::reasons(&app, scope(s)).await.unwrap();
    assert!(
        reasons
            .iter()
            .any(|r| r.reason_kind == "HUMAN_PIN" && r.human_selected)
    );
    assert!(
        reasons
            .iter()
            .any(|r| r.reason_kind == "UNRESOLVED_UNCERTAINTY")
    );
    assert!(reasons.iter().any(|r| r.reason_kind == "UNDECIDED_ACTION"));
    assert!(reasons.iter().all(|r| !r.automatically_ranked));
}

#[tokio::test]
async fn v059_v060_pin_and_unpin_preserve_history_without_hidden_priority() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let claim = make_claim(&app, s, "claim").await;
    let pin = PinEvent {
        event_id: Uuid::new_v4(),
        target_type: "CLAIM".into(),
        target_id: claim,
        kind: "PINNED".into(),
        reason: "human selected".into(),
        recorded_by: s.principal,
    };
    attention_control::append_pin_event(&app, scope(s), &pin)
        .await
        .unwrap();
    assert_eq!(
        attention_control::current_pins(&app, s.workspace, s.situation)
            .await
            .unwrap()
            .len(),
        1
    );
    attention_control::append_pin_event(
        &app,
        scope(s),
        &PinEvent {
            event_id: Uuid::new_v4(),
            kind: "UNPINNED".into(),
            reason: "done reviewing".into(),
            ..pin
        },
    )
    .await
    .unwrap();
    assert!(
        attention_control::current_pins(&app, s.workspace, s.situation)
            .await
            .unwrap()
            .is_empty()
    );
    let history: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_attention_pin_events WHERE workspace_id=$1 AND situation_id=$2 AND target_id=$3")
        .bind(s.workspace).bind(s.situation).bind(claim).fetch_one(&admin).await.unwrap();
    assert_eq!(history, 2);
}

#[tokio::test]
async fn v061_reanalysis_request_requires_exact_steering_event_and_does_not_run_analysis() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let claim = make_claim(&app, s, "claim").await;
    let review = Uuid::new_v4();
    analysis_claims::append_review(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        claim,
        review,
        "CHALLENGED",
        "please revisit",
        s.principal,
    )
    .await
    .unwrap();
    let request = ReanalysisRequest {
        request_id: Uuid::new_v4(),
        prior_analysis_id: s.analysis1,
        reason: "human challenged a claim".into(),
        steering_event_type: "CLAIM_REVIEW".into(),
        steering_event_id: review,
        requested_by: s.principal,
    };
    reanalysis_control::record_request(&app, s.workspace, s.situation, &request)
        .await
        .unwrap();
    let before: i64 = 2;
    let analyses: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2")
        .bind(s.workspace).bind(s.situation).fetch_one(&admin).await.unwrap();
    assert_eq!(analyses, before);
    let mut bad = request.clone();
    bad.request_id = Uuid::new_v4();
    bad.steering_event_id = Uuid::new_v4();
    assert!(
        reanalysis_control::record_request(&app, s.workspace, s.situation, &bad)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn v062_v065_review_checkpoint_reports_changes_without_implying_acceptance() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let checkpoint = ReviewCheckpoint {
        checkpoint_id: Uuid::new_v4(),
        observed_revision: 1,
        observed_analysis_id: Some(s.analysis1),
        note: Some("I looked here".into()),
        recorded_by: s.principal,
    };
    review_session::record_checkpoint(&app, s.workspace, s.situation, &checkpoint)
        .await
        .unwrap();
    let delta =
        review_session::since_checkpoint(&app, s.workspace, s.situation, checkpoint.checkpoint_id)
            .await
            .unwrap();
    assert_eq!(delta.revision_delta, 1);
    assert!(delta.changed_since_checkpoint);
    let history = review_session::checkpoint_history(&app, s.workspace, s.situation)
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert!(!history[0].review_means_acceptance);
}

#[tokio::test]
async fn v063_dashboard_keeps_attention_and_execution_under_human_control() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let claim = make_claim(&app, s, "claim").await;
    make_uncertainty(&app, s, Some(claim)).await;
    make_proposal(&app, s, Some(claim)).await;
    let d = review_session::dashboard(&app, s.workspace, s.situation)
        .await
        .unwrap();
    assert_eq!(d.analysis_count, 2);
    assert_eq!(d.unresolved_uncertainties, 1);
    assert_eq!(d.undecided_action_proposals, 1);
    assert!(!d.system_auto_prioritizes_attention);
    assert!(!d.authorization_means_execution);
}

#[tokio::test]
async fn v064_situation_export_and_hash_are_deterministic() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    make_claim(&app, s, "claim").await;
    let a = review_session::to_json(&app, s.workspace, s.situation)
        .await
        .unwrap();
    let b = review_session::to_json(&app, s.workspace, s.situation)
        .await
        .unwrap();
    assert_eq!(a, b);
    let h1 = review_session::sha256(&app, s.workspace, s.situation)
        .await
        .unwrap();
    let h2 = review_session::sha256(&app, s.workspace, s.situation)
        .await
        .unwrap();
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 64);
}

#[tokio::test]
async fn v066_v068_action_lifecycle_requires_acceptance_for_authorization_and_supports_revocation()
{
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let proposal = make_proposal(&app, s, None).await;
    let deferred = Uuid::new_v4();
    analysis_actions::append_decision(
        &app,
        scope(s),
        &ActionDecision {
            proposal_id: proposal,
            decision_id: deferred,
            kind: "DEFERRED".into(),
            note: "not yet".into(),
            decided_by: s.principal,
        },
    )
    .await
    .unwrap();
    assert!(
        action_authorization::append_event(
            &app,
            scope(s),
            &AuthorizationEvent {
                proposal_id: proposal,
                authorization_id: Uuid::new_v4(),
                kind: "AUTHORIZED".into(),
                accepted_decision_id: Some(deferred),
                scope: "send one draft".into(),
                recorded_by: s.principal
            }
        )
        .await
        .is_err()
    );
    let accepted = Uuid::new_v4();
    analysis_actions::append_decision(
        &app,
        scope(s),
        &ActionDecision {
            proposal_id: proposal,
            decision_id: accepted,
            kind: "ACCEPTED".into(),
            note: "I accept the proposal".into(),
            decided_by: s.principal,
        },
    )
    .await
    .unwrap();
    action_authorization::append_event(
        &app,
        scope(s),
        &AuthorizationEvent {
            proposal_id: proposal,
            authorization_id: Uuid::new_v4(),
            kind: "AUTHORIZED".into(),
            accepted_decision_id: Some(accepted),
            scope: "send one draft".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let lifecycle = action_authorization::lifecycle(&app, scope(s))
        .await
        .unwrap();
    assert_eq!(lifecycle.len(), 1);
    assert!(lifecycle[0].authorized);
    assert!(!lifecycle[0].executed);
    action_authorization::append_event(
        &app,
        scope(s),
        &AuthorizationEvent {
            proposal_id: proposal,
            authorization_id: Uuid::new_v4(),
            kind: "REVOKED".into(),
            accepted_decision_id: None,
            scope: "authorization withdrawn".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let status = action_authorization::statuses(&app, scope(s))
        .await
        .unwrap();
    assert_eq!(status.len(), 1);
    assert!(!status[0].authorized);
    assert!(!status[0].authorization_is_execution);
}

#[tokio::test]
async fn v069_v070_reanalysis_suggestion_never_runs_until_human_requests_and_request_still_is_not_execution()
 {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let suggestion = ReanalysisSuggestion {
        suggestion_id: Uuid::new_v4(),
        reason_kind: "CONTEXT_DELTA".into(),
        reason: "the Situation has changed".into(),
        proposed_by: s.principal,
    };
    reanalysis_control::record_suggestion(&app, scope(s), &suggestion)
        .await
        .unwrap();
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2")
        .bind(s.workspace).bind(s.situation).fetch_one(&admin).await.unwrap();
    let undecided = reanalysis_control::suggestion_statuses(&app, scope(s))
        .await
        .unwrap();
    assert_eq!(undecided[0].latest_decision, None);
    reanalysis_control::append_suggestion_decision(
        &app,
        scope(s),
        &SuggestionDecision {
            suggestion_id: suggestion.suggestion_id,
            decision_id: Uuid::new_v4(),
            kind: "REQUESTED".into(),
            note: "yes, reanalyze".into(),
            decided_by: s.principal,
        },
    )
    .await
    .unwrap();
    let statuses = reanalysis_control::suggestion_statuses(&app, scope(s))
        .await
        .unwrap();
    assert_eq!(statuses[0].latest_decision.as_deref(), Some("REQUESTED"));
    assert!(!statuses[0].requested_means_analysis_started);
    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_snapshots WHERE workspace_id=$1 AND situation_id=$2")
        .bind(s.workspace).bind(s.situation).fetch_one(&admin).await.unwrap();
    assert_eq!(before, after);
}
