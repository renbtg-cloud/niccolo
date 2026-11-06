use niccolo_evidence::analysis_claims::{self, NewClaim};
use niccolo_evidence::analysis_manifest;
use niccolo_evidence::analysis_structure::AnalysisScope;
use niccolo_evidence::attention_control::{self, PinEvent};
use niccolo_evidence::control_ledger;
use niccolo_evidence::inspection::{self, ClaimOutputSpan, ContextExclusion};
use niccolo_evidence::review_acknowledgement::{self, ReviewAcknowledgement};
use niccolo_evidence::review_package;
use niccolo_evidence::review_session::{self, ReviewCheckpoint};
use niccolo_evidence::situation_activity;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Seed {
    workspace: Uuid,
    situation: Uuid,
    principal: Uuid,
    analysis: Uuid,
    used_context: Uuid,
    unused_context: Uuid,
}

async fn pools() -> (PgPool, PgPool) {
    let admin_url = std::env::var("NICCOLO_PG_ADMIN_URL")
        .unwrap_or_else(|_| "postgres://oai@127.0.0.1:55432/niccolo_test".to_string());
    let app_url = std::env::var("NICCOLO_PG_SERVICE_URL").unwrap_or_else(|_| {
        "postgres://niccolo_service_test@127.0.0.1:55432/niccolo_test".to_string()
    });
    (
        PgPoolOptions::new()
            .max_connections(8)
            .connect(&admin_url)
            .await
            .unwrap(),
        PgPoolOptions::new()
            .max_connections(32)
            .connect(&app_url)
            .await
            .unwrap(),
    )
}

async fn seed(admin: &PgPool) -> Seed {
    let s = Seed {
        workspace: Uuid::new_v4(),
        situation: Uuid::new_v4(),
        principal: Uuid::new_v4(),
        analysis: Uuid::new_v4(),
        used_context: Uuid::new_v4(),
        unused_context: Uuid::new_v4(),
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
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'focus',$3,1)")
        .bind(s.workspace).bind(s.situation).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id) VALUES($1,$2,1,'focus',$3,$4)")
        .bind(s.workspace).bind(s.situation).bind("a".repeat(64)).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id) VALUES($1,$2,$3,1,$4,'understand',$5)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind("a".repeat(64)).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_outputs(workspace_id,situation_id,analysis_id,answer_text,coverage_note,published_by_principal_id) VALUES($1,$2,$3,'alpha beta','bounded',$4)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_contributions(workspace_id,situation_id,contribution_id,author_principal_id,kind,contribution_text,status) VALUES($1,$2,$3,$5,'USER_REPORTED','used context','ACTIVE'),($1,$2,$4,$5,'USER_REPORTED','unused context','ACTIVE')")
        .bind(s.workspace).bind(s.situation).bind(s.used_context).bind(s.unused_context).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revision_contributions(workspace_id,situation_id,revision,contribution_id) VALUES($1,$2,1,$3),($1,$2,1,$4)")
        .bind(s.workspace).bind(s.situation).bind(s.used_context).bind(s.unused_context).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_used_contributions(workspace_id,situation_id,analysis_id,input_revision,contribution_id,recorded_by_principal_id) VALUES($1,$2,$3,1,$4,$5)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(s.used_context).bind(s.principal).execute(admin).await.unwrap();
    s
}

fn scope(s: Seed) -> AnalysisScope {
    AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    }
}

async fn claim(app: &PgPool, s: Seed) -> Uuid {
    let id = Uuid::new_v4();
    analysis_claims::record_claim(
        app,
        s.workspace,
        s.situation,
        s.analysis,
        s.principal,
        NewClaim {
            claim_id: id,
            ordinal: 1,
            kind: "INFERENCE".into(),
            text: "alpha".into(),
            qualification: Some("provisional".into()),
        },
    )
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn v071_situation_activity_pages_without_duplicates() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let c = claim(&app, s).await;
    attention_control::append_pin_event(
        &app,
        scope(s),
        &PinEvent {
            event_id: Uuid::new_v4(),
            target_type: "CLAIM".into(),
            target_id: c,
            kind: "PINNED".into(),
            reason: "inspect".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    review_session::record_checkpoint(
        &app,
        s.workspace,
        s.situation,
        &ReviewCheckpoint {
            checkpoint_id: Uuid::new_v4(),
            observed_revision: 1,
            observed_analysis_id: Some(s.analysis),
            note: Some("looked".into()),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let p1 = situation_activity::page(&app, s.workspace, s.situation, None, 2)
        .await
        .unwrap();
    assert_eq!(p1.items.len(), 2);
    let p2 = situation_activity::page(&app, s.workspace, s.situation, p1.next.as_ref(), 2)
        .await
        .unwrap();
    for a in &p1.items {
        assert!(
            !p2.items
                .iter()
                .any(|b| b.event_type == a.event_type && b.object_id == a.object_id)
        );
    }
}

#[tokio::test]
async fn v072_human_control_state_never_claims_automatic_action() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    review_session::record_checkpoint(
        &app,
        s.workspace,
        s.situation,
        &ReviewCheckpoint {
            checkpoint_id: Uuid::new_v4(),
            observed_revision: 1,
            observed_analysis_id: Some(s.analysis),
            note: None,
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let state = review_session::human_control_state(&app, s.workspace, s.situation)
        .await
        .unwrap();
    assert_eq!(state.review_checkpoints, 1);
    assert!(!state.automatic_reanalysis_started);
    assert!(!state.external_actions_executed);
}

#[tokio::test]
async fn v073_context_exclusion_accounts_for_unused_input_without_calling_it_false() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    inspection::record_context_exclusion(
        &app,
        scope(s),
        &ContextExclusion {
            contribution_id: s.unused_context,
            reason_kind: "OUT_OF_SCOPE".into(),
            reason: "not needed for this goal".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    assert!(
        inspection::record_context_exclusion(
            &app,
            scope(s),
            &ContextExclusion {
                contribution_id: s.used_context,
                reason_kind: "OUT_OF_SCOPE".into(),
                reason: "contradictory API use should fail".into(),
                recorded_by: s.principal,
            }
        )
        .await
        .is_err()
    );
    let t = inspection::traceability(&app, scope(s)).await.unwrap();
    assert_eq!(t.frozen_context_count, 2);
    assert_eq!(t.used_context_count, 1);
    assert_eq!(t.excluded_context_count, 1);
    assert!(t.all_frozen_context_accounted);
    assert!(!t.accounting_means_truth);
}

#[tokio::test]
async fn v074_v075_claim_output_span_is_byte_bounded_and_traceable() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let c = claim(&app, s).await;
    inspection::record_output_span(
        &app,
        scope(s),
        &ClaimOutputSpan {
            claim_id: c,
            byte_start: 0,
            byte_end: 5,
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let t = inspection::traceability(&app, scope(s)).await.unwrap();
    assert_eq!(t.output_bytes, 10);
    assert_eq!(t.claim_count, 1);
    assert_eq!(t.claims_with_output_span, 1);
    assert!(t.all_claims_mapped_to_output);
    let c2 = Uuid::new_v4();
    analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        s.analysis,
        s.principal,
        NewClaim {
            claim_id: c2,
            ordinal: 2,
            kind: "INFERENCE".into(),
            text: "beta".into(),
            qualification: None,
        },
    )
    .await
    .unwrap();
    assert!(
        inspection::record_output_span(
            &app,
            scope(s),
            &ClaimOutputSpan {
                claim_id: c2,
                byte_start: 6,
                byte_end: 99,
                recorded_by: s.principal,
            }
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn v076_inspection_record_reports_traceability_not_correctness() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let c = claim(&app, s).await;
    inspection::record_context_exclusion(
        &app,
        scope(s),
        &ContextExclusion {
            contribution_id: s.unused_context,
            reason_kind: "OUT_OF_SCOPE".into(),
            reason: "not used".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    inspection::record_output_span(
        &app,
        scope(s),
        &ClaimOutputSpan {
            claim_id: c,
            byte_start: 0,
            byte_end: 5,
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    analysis_manifest::record_manifest(&app, s.workspace, s.situation, s.analysis, s.principal)
        .await
        .unwrap();
    let record = inspection::inspection_record(&app, scope(s)).await.unwrap();
    assert!(record.human_review_packet_has_traceability);
    assert!(!record.traceability_means_correctness);
}

#[tokio::test]
async fn v077_v078_acknowledgement_is_of_exact_hash_not_agreement() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    claim(&app, s).await;
    let hash = review_package::sha256(&app, scope(s)).await.unwrap();
    review_acknowledgement::record(
        &app,
        scope(s),
        &ReviewAcknowledgement {
            acknowledgement_id: Uuid::new_v4(),
            package_sha256: hash.clone(),
            kind: "VIEWED".into(),
            note: Some("I saw this version".into()),
            acknowledged_by: s.principal,
        },
    )
    .await
    .unwrap();
    let h = review_acknowledgement::history(&app, scope(s))
        .await
        .unwrap();
    assert_eq!(h[0].package_sha256, hash);
    assert!(!h[0].acknowledgement_means_agreement);
}

#[tokio::test]
async fn v079_v080_control_ledger_is_deterministic_hashed_and_explicit_about_boundaries() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let c = claim(&app, s).await;
    attention_control::append_pin_event(
        &app,
        scope(s),
        &PinEvent {
            event_id: Uuid::new_v4(),
            target_type: "CLAIM".into(),
            target_id: c,
            kind: "PINNED".into(),
            reason: "human chose this".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let a = control_ledger::to_json(&app, s.workspace, s.situation)
        .await
        .unwrap();
    let b = control_ledger::to_json(&app, s.workspace, s.situation)
        .await
        .unwrap();
    assert_eq!(a, b);
    let h = control_ledger::sha256(&app, s.workspace, s.situation)
        .await
        .unwrap();
    assert_eq!(h.len(), 64);
    let ledger = control_ledger::build(&app, s.workspace, s.situation)
        .await
        .unwrap();
    assert!(!ledger.boundaries.pin_means_auto_priority);
    assert!(!ledger.boundaries.acknowledgement_means_agreement);
    assert!(!ledger.boundaries.reanalysis_request_means_started);
    assert!(!ledger.boundaries.authorization_means_execution);
    assert!(!ledger.boundaries.context_exclusion_means_context_is_false);
}
