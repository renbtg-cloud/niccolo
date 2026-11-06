use niccolo_evidence::analysis_actions::{self, ActionProposal};
use niccolo_evidence::analysis_claims::{self, ClaimLineage, NewClaim};
use niccolo_evidence::analysis_compare::{self, AnalysisCompareError};
use niccolo_evidence::analysis_structure::{self, AnalysisScope, Uncertainty};
use niccolo_evidence::review_agenda;
use niccolo_evidence::review_governance::{self, HandoffReceipt, ScopeEvent, SessionDisposition};
use niccolo_evidence::review_workflow::{self, ReviewSessionScope};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Seed {
    workspace: Uuid,
    situation: Uuid,
    principal: Uuid,
    other_principal: Uuid,
    prior: Uuid,
    current: Uuid,
}

async fn pools() -> (PgPool, PgPool) {
    let admin_url = std::env::var("NICCOLO_NEXT_ADMIN_URL")
        .unwrap_or_else(|_| "postgres://oai@127.0.0.1:55432/niccolo_next_test".to_string());
    let app_url = std::env::var("NICCOLO_NEXT_APP_URL").unwrap_or_else(|_| {
        "postgres://niccolo_next_app@127.0.0.1:55432/niccolo_next_test".to_string()
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
        other_principal: Uuid::new_v4(),
        prior: Uuid::new_v4(),
        current: Uuid::new_v4(),
    };
    sqlx::query("INSERT INTO niccolo_auth.principals(principal_id) VALUES($1),($2)")
        .bind(s.principal)
        .bind(s.other_principal)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'w')")
        .bind(s.workspace)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'focus 2',$3,2)")
        .bind(s.workspace).bind(s.situation).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id,cause) VALUES($1,$2,1,'focus 1',$3,$5,'CREATED'),($1,$2,2,'focus 2',$4,$5,'FOCUS_CHANGED')")
        .bind(s.workspace).bind(s.situation).bind("a".repeat(64)).bind("b".repeat(64)).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id,reanalysis_of) VALUES($1,$2,$3,1,$5,'understand',$6,NULL),($1,$2,$4,2,$7,'reanalyze',$6,$3)")
        .bind(s.workspace).bind(s.situation).bind(s.prior).bind(s.current).bind("a".repeat(64)).bind(s.principal).bind("b".repeat(64)).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_outputs(workspace_id,situation_id,analysis_id,answer_text,coverage_note,published_by_principal_id) VALUES($1,$2,$3,'prior answer','bounded',$5),($1,$2,$4,'new answer','bounded',$5)")
        .bind(s.workspace).bind(s.situation).bind(s.prior).bind(s.current).bind(s.principal).execute(admin).await.unwrap();
    s
}

fn scope(s: Seed, analysis: Uuid) -> AnalysisScope {
    AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis,
    }
}

async fn claim(app: &PgPool, s: Seed, analysis: Uuid, ordinal: i32, text: &str) -> Uuid {
    let id = Uuid::new_v4();
    analysis_claims::record_claim(
        app,
        s.workspace,
        s.situation,
        analysis,
        s.principal,
        NewClaim {
            claim_id: id,
            ordinal,
            kind: "INFERENCE".into(),
            text: text.into(),
            qualification: Some("provisional".into()),
        },
    )
    .await
    .unwrap();
    id
}

async fn session(app: &PgPool, s: Seed) -> ReviewSessionScope {
    let id = Uuid::new_v4();
    review_workflow::start_session(
        app,
        s.workspace,
        s.situation,
        id,
        Some(s.current),
        "human review",
        s.principal,
    )
    .await
    .unwrap();
    ReviewSessionScope {
        workspace: s.workspace,
        situation: s.situation,
        session: id,
    }
}

#[tokio::test]
async fn v091_v092_scope_history_is_append_only_and_current_scope_is_derived() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let sess = session(&app, s).await;
    for (kind, event) in [
        ("CLAIMS", "SELECTED"),
        ("SOURCES", "SELECTED"),
        ("CLAIMS", "DESELECTED"),
    ] {
        review_governance::append_scope_event(
            &app,
            sess,
            &ScopeEvent {
                event_id: Uuid::new_v4(),
                scope_kind: kind.into(),
                event_kind: event.into(),
                reason: "human choice".into(),
                recorded_by: s.principal,
            },
        )
        .await
        .unwrap();
    }
    assert_eq!(
        review_governance::current_scope(&app, sess).await.unwrap(),
        vec!["SOURCES"]
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_review_scope_events WHERE workspace_id=$1 AND situation_id=$2 AND session_id=$3")
        .bind(s.workspace).bind(s.situation).bind(sess.session).fetch_one(&admin).await.unwrap();
    assert_eq!(count, 3);
}

#[tokio::test]
async fn v093_claim_page_is_bounded_and_keyset_pages_do_not_repeat() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    for i in 1..=5 {
        claim(&app, s, s.current, i, &format!("claim {i}")).await;
    }
    let p1 = analysis_compare::claim_page(&app, scope(s, s.current), None, 2)
        .await
        .unwrap();
    let p2 = analysis_compare::claim_page(&app, scope(s, s.current), p1.next_ordinal, 2)
        .await
        .unwrap();
    let p3 = analysis_compare::claim_page(&app, scope(s, s.current), p2.next_ordinal, 2)
        .await
        .unwrap();
    assert!(p1.bounded && p2.bounded && p3.bounded);
    assert_eq!((p1.items.len(), p2.items.len(), p3.items.len()), (2, 2, 1));
    let ids: std::collections::HashSet<_> = p1
        .items
        .iter()
        .chain(&p2.items)
        .chain(&p3.items)
        .map(|x| x.claim_id)
        .collect();
    assert_eq!(ids.len(), 5);
    assert!(
        analysis_compare::claim_page(&app, scope(s, s.current), None, 101)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn v094_handoff_receipt_binds_exact_hash_but_never_grants_agreement_or_authority() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let hash = "c".repeat(64);
    review_governance::record_handoff_receipt(
        &app,
        s.workspace,
        s.situation,
        s.current,
        &HandoffReceipt {
            receipt_id: Uuid::new_v4(),
            handoff_sha256: hash.clone(),
            kind: "RECEIVED".into(),
            note: Some("received exact bundle".into()),
            recipient: s.other_principal,
        },
    )
    .await
    .unwrap();
    let rows = review_governance::handoff_receipts(&app, s.workspace, s.situation, s.current)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].handoff_sha256, hash);
    assert!(!rows[0].receipt_means_agreement);
    assert!(!rows[0].receipt_grants_authority);
    let bad = HandoffReceipt {
        receipt_id: Uuid::new_v4(),
        handoff_sha256: "BAD".into(),
        kind: "RECEIVED".into(),
        note: None,
        recipient: s.other_principal,
    };
    assert!(
        review_governance::record_handoff_receipt(&app, s.workspace, s.situation, s.current, &bad)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn v095_comparison_rejects_pairs_that_are_not_direct_reanalysis() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let unrelated = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id) VALUES($1,$2,$3,2,$4,'unrelated',$5)")
        .bind(s.workspace).bind(s.situation).bind(unrelated).bind("b".repeat(64)).bind(s.principal).execute(&admin).await.unwrap();
    let err = analysis_compare::compare_direct_reanalysis(
        &app,
        s.workspace,
        s.situation,
        s.prior,
        unrelated,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AnalysisCompareError::NotDirectReanalysis));
}

#[tokio::test]
async fn v095_explicit_drop_is_distinct_from_absence_of_lineage() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let p1 = claim(&app, s, s.prior, 1, "kept").await;
    let p2 = claim(&app, s, s.prior, 2, "unmapped").await;
    let n1 = claim(&app, s, s.current, 1, "kept refined").await;
    analysis_claims::record_lineage(
        &app,
        s.workspace,
        s.situation,
        ClaimLineage {
            lineage_id: Uuid::new_v4(),
            prior_analysis_id: s.prior,
            prior_claim_id: p1,
            new_analysis_id: s.current,
            new_claim_id: Some(n1),
            relation: "REFINED".into(),
            rationale: "same issue, refined".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let before = analysis_compare::compare_direct_reanalysis(
        &app,
        s.workspace,
        s.situation,
        s.prior,
        s.current,
    )
    .await
    .unwrap();
    assert!(before.explicitly_dropped_prior_claims.is_empty());
    assert_eq!(before.prior_claims_without_lineage, vec![p2]);
    assert!(!before.absence_of_lineage_means_removed_or_added);
    analysis_claims::record_lineage(
        &app,
        s.workspace,
        s.situation,
        ClaimLineage {
            lineage_id: Uuid::new_v4(),
            prior_analysis_id: s.prior,
            prior_claim_id: p2,
            new_analysis_id: s.current,
            new_claim_id: None,
            relation: "DROPPED".into(),
            rationale: "explicitly no longer asserted".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let after = analysis_compare::compare_direct_reanalysis(
        &app,
        s.workspace,
        s.situation,
        s.prior,
        s.current,
    )
    .await
    .unwrap();
    assert_eq!(after.explicitly_dropped_prior_claims, vec![p2]);
    assert!(after.prior_claims_without_lineage.is_empty());
}

#[tokio::test]
async fn v096_lineage_completeness_is_measurable_but_not_correctness() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let p = claim(&app, s, s.prior, 1, "prior").await;
    let n = claim(&app, s, s.current, 1, "new").await;
    let before =
        analysis_compare::lineage_completeness(&app, s.workspace, s.situation, s.prior, s.current)
            .await
            .unwrap();
    assert!(!before.prior_complete && !before.new_complete);
    analysis_claims::record_lineage(
        &app,
        s.workspace,
        s.situation,
        ClaimLineage {
            lineage_id: Uuid::new_v4(),
            prior_analysis_id: s.prior,
            prior_claim_id: p,
            new_analysis_id: s.current,
            new_claim_id: Some(n),
            relation: "RETAINED".into(),
            rationale: "same claim".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let after =
        analysis_compare::lineage_completeness(&app, s.workspace, s.situation, s.prior, s.current)
            .await
            .unwrap();
    assert!(after.prior_complete && after.new_complete);
    assert!(!after.completeness_means_correctness);
}

async fn make_pending(admin: &PgPool, app: &PgPool, s: Seed, c: Uuid) -> (Uuid, Uuid, Uuid, Uuid) {
    let q = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.investigative_questions(workspace_id,question_id,question) VALUES($1,$2,'What remains open?')")
        .bind(s.workspace).bind(q).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_clarification_questions(workspace_id,situation_id,analysis_id,question_id,reason,proposed_by_principal_id) VALUES($1,$2,$3,$4,'open',$5)")
        .bind(s.workspace).bind(s.situation).bind(s.current).bind(q).bind(s.principal).execute(admin).await.unwrap();
    review_workflow::link_claim_question(
        app,
        scope(s, s.current),
        c,
        q,
        "OPEN_QUESTION",
        s.principal,
    )
    .await
    .unwrap();
    let condition = Uuid::new_v4();
    review_workflow::record_revision_condition(
        app,
        scope(s, s.current),
        c,
        condition,
        "OTHER",
        "if context changes",
        s.principal,
    )
    .await
    .unwrap();
    let uncertainty = Uuid::new_v4();
    analysis_structure::record_uncertainty(
        app,
        scope(s, s.current),
        &Uncertainty {
            uncertainty_id: uncertainty,
            claim_id: Some(c),
            kind: "UNKNOWN".into(),
            statement: "unknown".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let proposal = Uuid::new_v4();
    analysis_actions::record_proposal(
        app,
        scope(s, s.current),
        &ActionProposal {
            proposal_id: proposal,
            claim_id: Some(c),
            action_text: "ask user".into(),
            rationale: "human owns project".into(),
            risk_note: None,
            proposed_by: s.principal,
        },
    )
    .await
    .unwrap();
    (q, condition, uncertainty, proposal)
}

#[tokio::test]
async fn v097_human_pending_is_explicit_and_unranked() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let c = claim(&app, s, s.current, 1, "claim").await;
    let (q, condition, uncertainty, proposal) = make_pending(&admin, &app, s, c).await;
    let p = analysis_compare::human_pending(&app, scope(s, s.current))
        .await
        .unwrap();
    assert!(p.open_question_ids.contains(&q));
    assert!(p.open_condition_ids.contains(&condition));
    assert!(p.unresolved_uncertainty_ids.contains(&uncertainty));
    assert!(p.undecided_action_ids.contains(&proposal));
    assert!(!p.automatically_ranked);
}

#[tokio::test]
async fn v098_disposition_target_must_belong_to_session_analysis() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let current_claim = claim(&app, s, s.current, 1, "current").await;
    let foreign_claim = claim(&app, s, s.prior, 1, "prior").await;
    let sess = session(&app, s).await;
    let good = SessionDisposition {
        disposition_id: Uuid::new_v4(),
        target_type: "CLAIM".into(),
        target_id: current_claim,
        disposition: "DEFERRED".into(),
        note: "not now".into(),
        recorded_by: s.principal,
    };
    review_governance::record_disposition(&app, sess, &good)
        .await
        .unwrap();
    let bad = SessionDisposition {
        disposition_id: Uuid::new_v4(),
        target_id: foreign_claim,
        ..good
    };
    assert!(
        review_governance::record_disposition(&app, sess, &bad)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn v098_session_disposition_never_becomes_remembered_future_policy() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let c = claim(&app, s, s.current, 1, "claim").await;
    let sess = session(&app, s).await;
    review_governance::record_disposition(
        &app,
        sess,
        &SessionDisposition {
            disposition_id: Uuid::new_v4(),
            target_type: "CLAIM".into(),
            target_id: c,
            disposition: "OUT_OF_SCOPE_FOR_THIS_SESSION".into(),
            note: "this session only".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let d = review_governance::current_dispositions(&app, sess)
        .await
        .unwrap();
    assert_eq!(d.len(), 1);
    assert!(!d[0].remembered_future_policy);
}

#[tokio::test]
async fn v099_v100_agenda_is_deterministic_unranked_and_non_executing() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let c = claim(&app, s, s.current, 1, "claim").await;
    let _ = make_pending(&admin, &app, s, c).await;
    let sess = session(&app, s).await;
    review_governance::append_scope_event(
        &app,
        sess,
        &ScopeEvent {
            event_id: Uuid::new_v4(),
            scope_kind: "CLAIMS".into(),
            event_kind: "SELECTED".into(),
            reason: "review claims".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    review_governance::record_disposition(
        &app,
        sess,
        &SessionDisposition {
            disposition_id: Uuid::new_v4(),
            target_type: "CLAIM".into(),
            target_id: c,
            disposition: "KEEP_OPEN".into(),
            note: "needs human thought".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let a = review_agenda::build(&app, sess, scope(s, s.current))
        .await
        .unwrap();
    let b = review_agenda::build(&app, sess, scope(s, s.current))
        .await
        .unwrap();
    assert_eq!(
        review_agenda::canonical_json(&a).unwrap(),
        review_agenda::canonical_json(&b).unwrap()
    );
    assert_eq!(
        review_agenda::sha256(&a).unwrap(),
        review_agenda::sha256(&b).unwrap()
    );
    assert!(!a.automatically_ranked);
    assert!(!a.session_disposition_becomes_future_policy);
    assert!(!a.agenda_starts_analysis);
    assert!(!a.agenda_executes_actions);
    assert!(
        a.session_dispositions
            .iter()
            .all(|d| !d.remembered_future_policy)
    );
}
