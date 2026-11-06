use niccolo_evidence::analysis_claims::{
    self, ClaimError, ClaimLineage, ClarificationImpact, NewClaim,
};
use niccolo_evidence::analysis_export;
use niccolo_evidence::analysis_integrity::{self, IntegrityStatus};
use niccolo_evidence::analysis_manifest;
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Seed {
    workspace: Uuid,
    other_workspace: Uuid,
    situation: Uuid,
    principal: Uuid,
    other_principal: Uuid,
    analysis1: Uuid,
    analysis2: Uuid,
    evidence1: Uuid,
    evidence2: Uuid,
    context1: Uuid,
    answer_context: Uuid,
    question: Uuid,
    answer: Uuid,
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
        other_workspace: Uuid::new_v4(),
        situation: Uuid::new_v4(),
        principal: Uuid::new_v4(),
        other_principal: Uuid::new_v4(),
        analysis1: Uuid::new_v4(),
        analysis2: Uuid::new_v4(),
        evidence1: Uuid::new_v4(),
        evidence2: Uuid::new_v4(),
        context1: Uuid::new_v4(),
        answer_context: Uuid::new_v4(),
        question: Uuid::new_v4(),
        answer: Uuid::new_v4(),
    };
    sqlx::query("INSERT INTO niccolo_auth.principals(principal_id,display_name) VALUES($1,'user'),($2,'other')")
        .bind(s.principal).bind(s.other_principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,name) VALUES($1,'w'),($2,'other')")
        .bind(s.workspace)
        .bind(s.other_workspace)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'current focus',$3,2)")
        .bind(s.workspace).bind(s.situation).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id) VALUES($1,$2,1,'old focus',$3,$5),($1,$2,2,'current focus',$4,$5)")
        .bind(s.workspace).bind(s.situation).bind("a".repeat(64)).bind("b".repeat(64)).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id,reanalysis_of) VALUES($1,$2,$3,1,$5,'understand',$6,NULL),($1,$2,$4,2,$7,'understand after clarification',$6,$3)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis1).bind(s.analysis2).bind("a".repeat(64)).bind(s.principal).bind("b".repeat(64)).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_outputs(workspace_id,situation_id,analysis_id,answer_text,coverage_note,published_by_principal_id) VALUES($1,$2,$3,'answer one','partial coverage',$5),($1,$2,$4,'answer two','expanded coverage',$5)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis1).bind(s.analysis2).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_fragments(workspace_id,evidence_id,document_id,source_start,source_end) VALUES($1,$2,$4,0,10),($1,$3,$5,0,10)")
        .bind(s.workspace).bind(s.evidence1).bind(s.evidence2).bind(Uuid::new_v4()).bind(Uuid::new_v4()).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_examined_fragments(workspace_id,situation_id,analysis_id,evidence_id,recorded_by_principal_id) VALUES($1,$2,$3,$5,$7),($1,$2,$4,$5,$7),($1,$2,$4,$6,$7)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis1).bind(s.analysis2).bind(s.evidence1).bind(s.evidence2).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_contributions(workspace_id,situation_id,contribution_id,author_principal_id,kind,contribution_text,status) VALUES($1,$2,$3,$5,'USER_REPORTED','context one','ACTIVE'),($1,$2,$4,$5,'USER_REPORTED','clarification answer','ACTIVE')")
        .bind(s.workspace).bind(s.situation).bind(s.context1).bind(s.answer_context).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revision_contributions(workspace_id,situation_id,revision,contribution_id) VALUES($1,$2,1,$3),($1,$2,2,$3),($1,$2,2,$4)")
        .bind(s.workspace).bind(s.situation).bind(s.context1).bind(s.answer_context).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_used_contributions(workspace_id,situation_id,analysis_id,input_revision,contribution_id,recorded_by_principal_id) VALUES($1,$2,$3,1,$5,$7),($1,$2,$4,2,$5,$7),($1,$2,$4,2,$6,$7)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis1).bind(s.analysis2).bind(s.context1).bind(s.answer_context).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_clarification_questions(workspace_id,situation_id,analysis_id,question_id,reason,proposed_by_principal_id) VALUES($1,$2,$3,$4,'high information gain',$5)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis1).bind(s.question).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_clarification_answers(workspace_id,situation_id,analysis_id,question_id,answer_id,contribution_id,answered_by_principal_id,answer_revision) VALUES($1,$2,$3,$4,$5,$6,$7,2)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis1).bind(s.question).bind(s.answer).bind(s.answer_context).bind(s.principal).execute(admin).await.unwrap();
    s
}

fn claim(id: Uuid, ordinal: i32, text: &str) -> NewClaim {
    NewClaim {
        claim_id: id,
        ordinal,
        kind: "INFERENCE".into(),
        text: text.into(),
        qualification: Some("provisional".into()),
    }
}

#[tokio::test]
async fn v031_claims_are_ordered_immutable_and_idempotent() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let c1 = Uuid::new_v4();
    let c2 = Uuid::new_v4();
    analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        s.principal,
        claim(c2, 2, "second"),
    )
    .await
    .unwrap();
    analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        s.principal,
        claim(c1, 1, "first"),
    )
    .await
    .unwrap();
    analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        s.principal,
        claim(c1, 1, "first"),
    )
    .await
    .unwrap();
    let rows = analysis_claims::list_claims(&app, s.workspace, s.situation, s.analysis1)
        .await
        .unwrap();
    assert_eq!(
        rows.iter().map(|c| c.claim_id).collect::<Vec<_>>(),
        vec![c1, c2]
    );
    let err = analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        s.principal,
        claim(c1, 1, "changed"),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ClaimError::Conflict));
}

#[tokio::test]
async fn v031_concurrent_same_claim_converges() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let id = Uuid::new_v4();
    let mut joins = Vec::new();
    for _ in 0..24 {
        let p = app.clone();
        joins.push(tokio::spawn(async move {
            analysis_claims::record_claim(
                &p,
                s.workspace,
                s.situation,
                s.analysis1,
                s.principal,
                claim(id, 1, "same"),
            )
            .await
        }));
    }
    for j in joins {
        j.await.unwrap().unwrap();
    }
    assert_eq!(
        analysis_claims::list_claims(&app, s.workspace, s.situation, s.analysis1)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn v032_support_links_require_inputs_actually_examined_or_used() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let cid = Uuid::new_v4();
    analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        s.principal,
        claim(cid, 1, "c"),
    )
    .await
    .unwrap();
    analysis_claims::link_fragment(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        cid,
        s.evidence1,
        "SUPPORTS_AS_USED",
        s.principal,
    )
    .await
    .unwrap();
    analysis_claims::link_contribution(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        cid,
        s.context1,
        "INFORMS",
        s.principal,
    )
    .await
    .unwrap();
    assert!(
        analysis_claims::link_fragment(
            &app,
            s.workspace,
            s.situation,
            s.analysis1,
            cid,
            s.evidence2,
            "SUPPORTS_AS_USED",
            s.principal
        )
        .await
        .is_err()
    );
    assert!(
        analysis_claims::link_contribution(
            &app,
            s.workspace,
            s.situation,
            s.analysis1,
            cid,
            s.answer_context,
            "INFORMS",
            s.principal
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn v033_human_reviews_append_without_rewriting_claim() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let cid = Uuid::new_v4();
    analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        s.principal,
        claim(cid, 1, "still here"),
    )
    .await
    .unwrap();
    analysis_claims::append_review(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        cid,
        Uuid::new_v4(),
        "CHALLENGED",
        "I disagree",
        s.principal,
    )
    .await
    .unwrap();
    analysis_claims::append_review(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        cid,
        Uuid::new_v4(),
        "INSUFFICIENT_EVIDENCE",
        "show me more",
        s.principal,
    )
    .await
    .unwrap();
    let reviews = analysis_claims::list_reviews(&app, s.workspace, s.situation, s.analysis1, cid)
        .await
        .unwrap();
    assert_eq!(reviews.len(), 2);
    assert_eq!(
        analysis_claims::list_claims(&app, s.workspace, s.situation, s.analysis1)
            .await
            .unwrap()[0]
            .text,
        "still here"
    );
}

#[tokio::test]
async fn v034_lineage_is_bound_to_direct_reanalysis() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let old = Uuid::new_v4();
    let new = Uuid::new_v4();
    let dropped = Uuid::new_v4();
    analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        s.principal,
        claim(old, 1, "old"),
    )
    .await
    .unwrap();
    analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        s.principal,
        claim(dropped, 2, "drop me"),
    )
    .await
    .unwrap();
    analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        s.analysis2,
        s.principal,
        claim(new, 1, "refined"),
    )
    .await
    .unwrap();
    analysis_claims::record_lineage(
        &app,
        s.workspace,
        s.situation,
        ClaimLineage {
            lineage_id: Uuid::new_v4(),
            prior_analysis_id: s.analysis1,
            prior_claim_id: old,
            new_analysis_id: s.analysis2,
            new_claim_id: Some(new),
            relation: "REFINED".into(),
            rationale: "new context".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    analysis_claims::record_lineage(
        &app,
        s.workspace,
        s.situation,
        ClaimLineage {
            lineage_id: Uuid::new_v4(),
            prior_analysis_id: s.analysis1,
            prior_claim_id: dropped,
            new_analysis_id: s.analysis2,
            new_claim_id: None,
            relation: "DROPPED".into(),
            rationale: "not retained".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let bad = analysis_claims::record_lineage(
        &app,
        s.workspace,
        s.situation,
        ClaimLineage {
            lineage_id: Uuid::new_v4(),
            prior_analysis_id: s.analysis2,
            prior_claim_id: new,
            new_analysis_id: s.analysis1,
            new_claim_id: Some(old),
            relation: "REVERSED".into(),
            rationale: "wrong direction".into(),
            recorded_by: s.principal,
        },
    )
    .await;
    assert!(bad.is_err());
}

#[tokio::test]
async fn v035_clarification_impact_requires_answer_context_used_by_target() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let cid = Uuid::new_v4();
    analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        s.analysis2,
        s.principal,
        claim(cid, 1, "after answer"),
    )
    .await
    .unwrap();
    analysis_claims::record_clarification_impact(
        &app,
        s.workspace,
        s.situation,
        ClarificationImpact {
            impact_id: Uuid::new_v4(),
            source_analysis_id: s.analysis1,
            question_id: s.question,
            answer_id: s.answer,
            contribution_id: s.answer_context,
            target_analysis_id: s.analysis2,
            target_claim_id: cid,
            recorded_effect: "QUALIFIES".into(),
            note: "answer was used as reported context".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let bad = analysis_claims::record_clarification_impact(
        &app,
        s.workspace,
        s.situation,
        ClarificationImpact {
            impact_id: Uuid::new_v4(),
            source_analysis_id: s.analysis1,
            question_id: s.question,
            answer_id: s.answer,
            contribution_id: s.context1,
            target_analysis_id: s.analysis2,
            target_claim_id: cid,
            recorded_effect: "INFORMS".into(),
            note: "mismatched answer contribution".into(),
            recorded_by: s.principal,
        },
    )
    .await;
    assert!(bad.is_err());
}

async fn populate_manifestable(app: &PgPool, s: Seed) -> Uuid {
    let cid = Uuid::new_v4();
    analysis_claims::record_claim(
        app,
        s.workspace,
        s.situation,
        s.analysis1,
        s.principal,
        claim(cid, 1, "manifested claim"),
    )
    .await
    .unwrap();
    analysis_claims::link_fragment(
        app,
        s.workspace,
        s.situation,
        s.analysis1,
        cid,
        s.evidence1,
        "SUPPORTS_AS_USED",
        s.principal,
    )
    .await
    .unwrap();
    analysis_claims::link_contribution(
        app,
        s.workspace,
        s.situation,
        s.analysis1,
        cid,
        s.context1,
        "INFORMS",
        s.principal,
    )
    .await
    .unwrap();
    cid
}

#[tokio::test]
async fn v036_manifest_is_stable_across_later_human_review() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let cid = populate_manifestable(&app, s).await;
    let before = analysis_manifest::record_manifest(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        s.principal,
    )
    .await
    .unwrap();
    assert_eq!(before.len(), 64);
    analysis_claims::append_review(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        cid,
        Uuid::new_v4(),
        "COMMENT",
        "later human note",
        s.principal,
    )
    .await
    .unwrap();
    let after = analysis_manifest::compute_hash(&app, s.workspace, s.situation, s.analysis1)
        .await
        .unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn v036_concurrent_manifest_recording_converges() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    populate_manifestable(&app, s).await;
    let mut joins = Vec::new();
    for _ in 0..16 {
        let p = app.clone();
        joins.push(tokio::spawn(async move {
            analysis_manifest::record_manifest(
                &p,
                s.workspace,
                s.situation,
                s.analysis1,
                s.principal,
            )
            .await
        }));
    }
    let mut hashes = Vec::new();
    for j in joins {
        hashes.push(j.await.unwrap().unwrap());
    }
    assert!(hashes.iter().all(|h| h == &hashes[0]));
}

#[tokio::test]
async fn v037_export_is_deterministic_and_contains_append_only_review() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let cid = populate_manifestable(&app, s).await;
    analysis_manifest::record_manifest(&app, s.workspace, s.situation, s.analysis1, s.principal)
        .await
        .unwrap();
    analysis_claims::append_review(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        cid,
        Uuid::new_v4(),
        "COMMENT",
        "portable note",
        s.principal,
    )
    .await
    .unwrap();
    let a = analysis_export::to_json(&app, s.workspace, s.situation, s.analysis1)
        .await
        .unwrap();
    let b = analysis_export::to_json(&app, s.workspace, s.situation, s.analysis1)
        .await
        .unwrap();
    assert_eq!(a, b);
    assert!(a.contains("portable note"));
    assert!(a.contains("niccolo.analysis-export"));
    assert!(!a.contains("bearer_token"));
}

#[tokio::test]
async fn v038_integrity_detects_core_tampering_but_not_later_review() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let cid = populate_manifestable(&app, s).await;
    assert!(matches!(
        analysis_integrity::verify(&app, s.workspace, s.situation, s.analysis1)
            .await
            .unwrap(),
        IntegrityStatus::MissingManifest { .. }
    ));
    analysis_manifest::record_manifest(&app, s.workspace, s.situation, s.analysis1, s.principal)
        .await
        .unwrap();
    analysis_claims::append_review(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        cid,
        Uuid::new_v4(),
        "COMMENT",
        "post-seal note",
        s.principal,
    )
    .await
    .unwrap();
    assert!(matches!(
        analysis_integrity::verify(&app, s.workspace, s.situation, s.analysis1)
            .await
            .unwrap(),
        IntegrityStatus::Verified { .. }
    ));
    sqlx::query("UPDATE niccolo.situation_analysis_claims SET claim_text='tampered by privileged test harness' WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4")
        .bind(s.workspace).bind(s.situation).bind(s.analysis1).bind(cid).execute(&admin).await.unwrap();
    assert!(matches!(
        analysis_integrity::verify(&app, s.workspace, s.situation, s.analysis1)
            .await
            .unwrap(),
        IntegrityStatus::Mismatch { .. }
    ));
}

#[tokio::test]
async fn cross_workspace_reads_do_not_leak_claims() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let cid = Uuid::new_v4();
    analysis_claims::record_claim(
        &app,
        s.workspace,
        s.situation,
        s.analysis1,
        s.principal,
        claim(cid, 1, "private"),
    )
    .await
    .unwrap();
    let rows = analysis_claims::list_claims(&app, s.other_workspace, s.situation, s.analysis1)
        .await
        .unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn invalid_semantics_fail_before_database_write() {
    let (admin, app) = pools().await;
    let s = seed(&admin).await;
    let bad = NewClaim {
        claim_id: Uuid::new_v4(),
        ordinal: 0,
        kind: "CERTAIN_TRUTH".into(),
        text: "x".into(),
        qualification: None,
    };
    assert!(matches!(
        analysis_claims::record_claim(
            &app,
            s.workspace,
            s.situation,
            s.analysis1,
            s.principal,
            bad
        )
        .await
        .unwrap_err(),
        ClaimError::Invalid
    ));
}

mod milestones_039_046 {
    use super::*;
    use niccolo_evidence::analysis_actions::{self, ActionDecision, ActionProposal};
    use niccolo_evidence::analysis_structure::{
        self, AnalysisScope, ClaimRelation, Uncertainty, UncertaintyEvent,
    };
    use niccolo_evidence::analysis_timeline;

    fn scope(s: Seed, analysis: Uuid) -> AnalysisScope {
        AnalysisScope {
            workspace: s.workspace,
            situation: s.situation,
            analysis,
        }
    }

    async fn two_claims(app: &PgPool, s: Seed) -> (Uuid, Uuid) {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        analysis_claims::record_claim(
            app,
            s.workspace,
            s.situation,
            s.analysis1,
            s.principal,
            claim(a, 1, "A"),
        )
        .await
        .unwrap();
        analysis_claims::record_claim(
            app,
            s.workspace,
            s.situation,
            s.analysis1,
            s.principal,
            claim(b, 2, "B"),
        )
        .await
        .unwrap();
        (a, b)
    }

    #[tokio::test]
    async fn v039_claim_relations_are_explicit_and_same_analysis() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let (a, b) = two_claims(&app, s).await;
        let r = ClaimRelation {
            relation_id: Uuid::new_v4(),
            source_claim_id: a,
            target_claim_id: b,
            relation: "QUALIFIES".into(),
            rationale: "A narrows B".into(),
            recorded_by: s.principal,
        };
        analysis_structure::record_relation(&app, scope(s, s.analysis1), &r)
            .await
            .unwrap();
        analysis_structure::record_relation(&app, scope(s, s.analysis1), &r)
            .await
            .unwrap();
        let rows = analysis_structure::list_relations(&app, scope(s, s.analysis1))
            .await
            .unwrap();
        assert_eq!(rows, vec![r]);
        let foreign = Uuid::new_v4();
        analysis_claims::record_claim(
            &app,
            s.workspace,
            s.situation,
            s.analysis2,
            s.principal,
            claim(foreign, 1, "later"),
        )
        .await
        .unwrap();
        let bad = ClaimRelation {
            relation_id: Uuid::new_v4(),
            source_claim_id: a,
            target_claim_id: foreign,
            relation: "SUPPORTS".into(),
            rationale: "wrong analysis".into(),
            recorded_by: s.principal,
        };
        assert!(
            analysis_structure::record_relation(&app, scope(s, s.analysis1), &bad)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn v040_dependency_cycles_are_rejected() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let (a, b) = two_claims(&app, s).await;
        let c = Uuid::new_v4();
        analysis_claims::record_claim(
            &app,
            s.workspace,
            s.situation,
            s.analysis1,
            s.principal,
            claim(c, 3, "C"),
        )
        .await
        .unwrap();
        for (src, dst) in [(a, b), (b, c)] {
            analysis_structure::record_relation(
                &app,
                scope(s, s.analysis1),
                &ClaimRelation {
                    relation_id: Uuid::new_v4(),
                    source_claim_id: src,
                    target_claim_id: dst,
                    relation: "DEPENDS_ON".into(),
                    rationale: "dependency".into(),
                    recorded_by: s.principal,
                },
            )
            .await
            .unwrap();
        }
        let cycle = ClaimRelation {
            relation_id: Uuid::new_v4(),
            source_claim_id: c,
            target_claim_id: a,
            relation: "DEPENDS_ON".into(),
            rationale: "cycle".into(),
            recorded_by: s.principal,
        };
        assert!(
            analysis_structure::record_relation(&app, scope(s, s.analysis1), &cycle)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn v041_uncertainties_are_explicit_immutable_records() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let (a, _) = two_claims(&app, s).await;
        let u = Uncertainty {
            uncertainty_id: Uuid::new_v4(),
            claim_id: Some(a),
            kind: "MISSING_EVIDENCE".into(),
            statement: "Need an independent source".into(),
            recorded_by: s.principal,
        };
        analysis_structure::record_uncertainty(&app, scope(s, s.analysis1), &u)
            .await
            .unwrap();
        analysis_structure::record_uncertainty(&app, scope(s, s.analysis1), &u)
            .await
            .unwrap();
        let changed = Uncertainty {
            statement: "silently changed".into(),
            ..u.clone()
        };
        assert!(
            analysis_structure::record_uncertainty(&app, scope(s, s.analysis1), &changed)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn v042_uncertainty_events_append_without_rewriting_origin() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let u = Uncertainty {
            uncertainty_id: Uuid::new_v4(),
            claim_id: None,
            kind: "AMBIGUITY".into(),
            statement: "Which Roberto?".into(),
            recorded_by: s.principal,
        };
        analysis_structure::record_uncertainty(&app, scope(s, s.analysis1), &u)
            .await
            .unwrap();
        let invalid = UncertaintyEvent {
            uncertainty_id: u.uncertainty_id,
            event_id: Uuid::new_v4(),
            kind: "CLARIFIED".into(),
            note: "answered".into(),
            target_analysis_id: None,
            recorded_by: s.principal,
        };
        assert!(
            analysis_structure::append_uncertainty_event(&app, scope(s, s.analysis1), &invalid)
                .await
                .is_err()
        );
        let valid = UncertaintyEvent {
            target_analysis_id: Some(s.analysis2),
            ..invalid
        };
        analysis_structure::append_uncertainty_event(&app, scope(s, s.analysis1), &valid)
            .await
            .unwrap();
        let original:String=sqlx::query_scalar("SELECT statement FROM niccolo.situation_analysis_uncertainties WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND uncertainty_id=$4")
            .bind(s.workspace).bind(s.situation).bind(s.analysis1).bind(u.uncertainty_id).fetch_one(&admin).await.unwrap();
        assert_eq!(original, "Which Roberto?");
    }

    #[tokio::test]
    async fn v043_action_proposal_is_not_execution_or_authorization() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let (a, _) = two_claims(&app, s).await;
        let p = ActionProposal {
            proposal_id: Uuid::new_v4(),
            claim_id: Some(a),
            action_text: "Ask Ana for the original email".into(),
            rationale: "Reduce ambiguity".into(),
            risk_note: Some("May be awkward".into()),
            proposed_by: s.principal,
        };
        analysis_actions::record_proposal(&app, scope(s, s.analysis1), &p)
            .await
            .unwrap();
        let report = analysis_actions::agency_report(&app, scope(s, s.analysis1))
            .await
            .unwrap();
        assert_eq!(report.proposal_count, 1);
        assert_eq!(report.decision_count, 0);
        assert_eq!(report.undecided_count, 1);
        assert!(!report.accepted_means_executed);
    }

    #[tokio::test]
    async fn v044_human_action_decisions_are_append_only() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let p = ActionProposal {
            proposal_id: Uuid::new_v4(),
            claim_id: None,
            action_text: "Wait one day".into(),
            rationale: "Avoid premature action".into(),
            risk_note: None,
            proposed_by: s.principal,
        };
        analysis_actions::record_proposal(&app, scope(s, s.analysis1), &p)
            .await
            .unwrap();
        for (kind, note) in [("DEFERRED", "not now"), ("ACCEPTED", "now I want it")] {
            analysis_actions::append_decision(
                &app,
                scope(s, s.analysis1),
                &ActionDecision {
                    proposal_id: p.proposal_id,
                    decision_id: Uuid::new_v4(),
                    kind: kind.into(),
                    note: note.into(),
                    decided_by: s.principal,
                },
            )
            .await
            .unwrap();
        }
        let report = analysis_actions::agency_report(&app, scope(s, s.analysis1))
            .await
            .unwrap();
        assert_eq!(report.decision_count, 2);
        assert_eq!(report.deferred_count, 1);
        assert_eq!(report.accepted_count, 1);
        assert_eq!(report.undecided_count, 0);
        assert!(!report.accepted_means_executed);
    }

    #[tokio::test]
    async fn v045_timeline_exposes_analysis_and_human_steering() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let (a, _) = two_claims(&app, s).await;
        analysis_claims::append_review(
            &app,
            s.workspace,
            s.situation,
            s.analysis1,
            a,
            Uuid::new_v4(),
            "COMMENT",
            "human note",
            s.principal,
        )
        .await
        .unwrap();
        let u = Uncertainty {
            uncertainty_id: Uuid::new_v4(),
            claim_id: Some(a),
            kind: "UNKNOWN".into(),
            statement: "Unknown motive".into(),
            recorded_by: s.principal,
        };
        analysis_structure::record_uncertainty(&app, scope(s, s.analysis1), &u)
            .await
            .unwrap();
        let events = analysis_timeline::list(&app, scope(s, s.analysis1))
            .await
            .unwrap();
        assert!(
            events
                .iter()
                .any(|e| e.event_type == "CLAIM" && e.object_id == a)
        );
        assert!(
            events
                .iter()
                .any(|e| e.event_type == "CLAIM_REVIEW" && e.detail == "human note")
        );
        assert!(
            events
                .iter()
                .any(|e| e.event_type == "UNCERTAINTY" && e.object_id == u.uncertainty_id)
        );
    }

    #[tokio::test]
    async fn v046_agency_report_never_equates_acceptance_with_execution() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        for i in 0..3 {
            let p = ActionProposal {
                proposal_id: Uuid::new_v4(),
                claim_id: None,
                action_text: format!("proposal {i}"),
                rationale: "user decides".into(),
                risk_note: None,
                proposed_by: s.principal,
            };
            analysis_actions::record_proposal(&app, scope(s, s.analysis1), &p)
                .await
                .unwrap();
            if i == 0 {
                analysis_actions::append_decision(
                    &app,
                    scope(s, s.analysis1),
                    &ActionDecision {
                        proposal_id: p.proposal_id,
                        decision_id: Uuid::new_v4(),
                        kind: "ACCEPTED".into(),
                        note: "accepted only".into(),
                        decided_by: s.principal,
                    },
                )
                .await
                .unwrap();
            }
            if i == 1 {
                analysis_actions::append_decision(
                    &app,
                    scope(s, s.analysis1),
                    &ActionDecision {
                        proposal_id: p.proposal_id,
                        decision_id: Uuid::new_v4(),
                        kind: "REJECTED".into(),
                        note: "no".into(),
                        decided_by: s.principal,
                    },
                )
                .await
                .unwrap();
            }
        }
        let r = analysis_actions::agency_report(&app, scope(s, s.analysis1))
            .await
            .unwrap();
        assert_eq!(
            (
                r.proposal_count,
                r.accepted_count,
                r.rejected_count,
                r.undecided_count
            ),
            (3, 1, 1, 1)
        );
        assert!(!r.accepted_means_executed);
    }
}

mod milestones_047_056 {
    use super::*;
    use niccolo_evidence::analysis_actions::{self, ActionProposal};
    use niccolo_evidence::analysis_read_models;
    use niccolo_evidence::analysis_structure::{self, AnalysisScope, Uncertainty};
    use niccolo_evidence::review_package;

    fn scope(s: Seed, analysis: Uuid) -> AnalysisScope {
        AnalysisScope {
            workspace: s.workspace,
            situation: s.situation,
            analysis,
        }
    }

    async fn add_claim_with_support(app: &PgPool, s: Seed) -> Uuid {
        let cid = Uuid::new_v4();
        analysis_claims::record_claim(
            app,
            s.workspace,
            s.situation,
            s.analysis1,
            s.principal,
            claim(cid, 1, "supported claim"),
        )
        .await
        .unwrap();
        analysis_claims::link_fragment(
            app,
            s.workspace,
            s.situation,
            s.analysis1,
            cid,
            s.evidence1,
            "SUPPORTS_AS_USED",
            s.principal,
        )
        .await
        .unwrap();
        analysis_claims::link_contribution(
            app,
            s.workspace,
            s.situation,
            s.analysis1,
            cid,
            s.context1,
            "QUALIFIES",
            s.principal,
        )
        .await
        .unwrap();
        cid
    }

    #[tokio::test]
    async fn v047_claim_support_summary_does_not_fake_independence() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let cid = add_claim_with_support(&app, s).await;
        let r = analysis_read_models::claim_support(&app, scope(s, s.analysis1), cid)
            .await
            .unwrap();
        assert_eq!((r.supports_as_used, r.qualifies_context), (1, 1));
        assert!(!r.link_counts_are_independent_evidence_count);
    }

    #[tokio::test]
    async fn v048_review_summary_counts_without_mutating_claim() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let cid = add_claim_with_support(&app, s).await;
        for (kind, note) in [
            ("CHALLENGED", "why?"),
            ("INSUFFICIENT_EVIDENCE", "more please"),
            ("COMMENT", "noted"),
        ] {
            analysis_claims::append_review(
                &app,
                s.workspace,
                s.situation,
                s.analysis1,
                cid,
                Uuid::new_v4(),
                kind,
                note,
                s.principal,
            )
            .await
            .unwrap();
        }
        let r = analysis_read_models::claim_reviews(&app, scope(s, s.analysis1), cid)
            .await
            .unwrap();
        assert_eq!(
            (r.challenged, r.insufficient_evidence, r.comments),
            (1, 1, 1)
        );
        assert!(!r.review_mutates_claim);
    }

    #[tokio::test]
    async fn v049_reanalysis_summary_uses_explicit_lineage_only() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let old = Uuid::new_v4();
        let new = Uuid::new_v4();
        analysis_claims::record_claim(
            &app,
            s.workspace,
            s.situation,
            s.analysis1,
            s.principal,
            claim(old, 1, "old"),
        )
        .await
        .unwrap();
        analysis_claims::record_claim(
            &app,
            s.workspace,
            s.situation,
            s.analysis2,
            s.principal,
            claim(new, 1, "new"),
        )
        .await
        .unwrap();
        analysis_claims::record_lineage(
            &app,
            s.workspace,
            s.situation,
            ClaimLineage {
                lineage_id: Uuid::new_v4(),
                prior_analysis_id: s.analysis1,
                prior_claim_id: old,
                new_analysis_id: s.analysis2,
                new_claim_id: Some(new),
                relation: "REFINED".into(),
                rationale: "new input".into(),
                recorded_by: s.principal,
            },
        )
        .await
        .unwrap();
        let r = analysis_read_models::reanalysis_changes(&app, scope(s, s.analysis2))
            .await
            .unwrap();
        assert_eq!(r.prior_analysis_id, s.analysis1);
        assert_eq!(r.refined, 1);
        assert_eq!(r.reversed, 0);
    }

    #[tokio::test]
    async fn v050_clarification_effect_summary_is_analysis_scoped() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let cid = Uuid::new_v4();
        analysis_claims::record_claim(
            &app,
            s.workspace,
            s.situation,
            s.analysis2,
            s.principal,
            claim(cid, 1, "after clarification"),
        )
        .await
        .unwrap();
        analysis_claims::record_clarification_impact(
            &app,
            s.workspace,
            s.situation,
            ClarificationImpact {
                impact_id: Uuid::new_v4(),
                source_analysis_id: s.analysis1,
                question_id: s.question,
                answer_id: s.answer,
                contribution_id: s.answer_context,
                target_analysis_id: s.analysis2,
                target_claim_id: cid,
                recorded_effect: "RESOLVES_FOR_THIS_ANALYSIS".into(),
                note: "only here".into(),
                recorded_by: s.principal,
            },
        )
        .await
        .unwrap();
        let r = analysis_read_models::clarification_effects(&app, scope(s, s.analysis2))
            .await
            .unwrap();
        assert_eq!(r.resolves_for_this_analysis, 1);
        assert_eq!(r.informs, 0);
    }

    #[tokio::test]
    async fn v051_analysis_index_marks_current_vs_historical_inputs() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let rows = analysis_read_models::analysis_index(&app, s.workspace, s.situation)
            .await
            .unwrap();
        let a1 = rows.iter().find(|x| x.analysis_id == s.analysis1).unwrap();
        let a2 = rows.iter().find(|x| x.analysis_id == s.analysis2).unwrap();
        assert!(!a1.input_is_current);
        assert!(a2.input_is_current);
        assert!(a1.output_present && a2.output_present);
    }

    #[tokio::test]
    async fn v052_attention_summary_lists_without_auto_prioritizing() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let u = Uncertainty {
            uncertainty_id: Uuid::new_v4(),
            claim_id: None,
            kind: "UNKNOWN".into(),
            statement: "still unknown".into(),
            recorded_by: s.principal,
        };
        analysis_structure::record_uncertainty(&app, scope(s, s.analysis1), &u)
            .await
            .unwrap();
        let p = ActionProposal {
            proposal_id: Uuid::new_v4(),
            claim_id: None,
            action_text: "ask later".into(),
            rationale: "user choice".into(),
            risk_note: None,
            proposed_by: s.principal,
        };
        analysis_actions::record_proposal(&app, scope(s, s.analysis1), &p)
            .await
            .unwrap();
        let r = analysis_read_models::attention(&app, scope(s, s.analysis1))
            .await
            .unwrap();
        assert_eq!(
            (r.unresolved_uncertainties, r.undecided_action_proposals),
            (1, 1)
        );
        assert!(!r.automatically_prioritized);
    }

    #[tokio::test]
    async fn v053_human_steering_is_counted_not_promoted_to_future_policy() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let cid = add_claim_with_support(&app, s).await;
        analysis_claims::append_review(
            &app,
            s.workspace,
            s.situation,
            s.analysis1,
            cid,
            Uuid::new_v4(),
            "COMMENT",
            "human steering",
            s.principal,
        )
        .await
        .unwrap();
        let r = analysis_read_models::human_steering(&app, scope(s, s.analysis1))
            .await
            .unwrap();
        assert_eq!(r.clarification_answers, 1);
        assert_eq!(r.claim_reviews, 1);
        assert!(!r.steering_is_future_policy);
    }

    #[tokio::test]
    async fn v054_review_package_is_deterministic_and_human_legible() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        add_claim_with_support(&app, s).await;
        let a = review_package::to_json(&app, scope(s, s.analysis1))
            .await
            .unwrap();
        let b = review_package::to_json(&app, scope(s, s.analysis1))
            .await
            .unwrap();
        assert_eq!(a, b);
        assert!(a.contains("niccolo.human-review-package"));
        assert!(a.contains("accepted_proposal_means_executed"));
    }

    #[tokio::test]
    async fn v055_review_package_hash_changes_when_append_only_review_changes() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let cid = add_claim_with_support(&app, s).await;
        let before = review_package::sha256(&app, scope(s, s.analysis1))
            .await
            .unwrap();
        analysis_claims::append_review(
            &app,
            s.workspace,
            s.situation,
            s.analysis1,
            cid,
            Uuid::new_v4(),
            "COMMENT",
            "later review",
            s.principal,
        )
        .await
        .unwrap();
        let after = review_package::sha256(&app, scope(s, s.analysis1))
            .await
            .unwrap();
        assert_ne!(before, after);
        assert_eq!(before.len(), 64);
        assert_eq!(after.len(), 64);
    }

    #[tokio::test]
    async fn v056_governance_boundaries_are_explicit_in_export() {
        let (admin, app) = pools().await;
        let s = seed(&admin).await;
        let pkg = review_package::build(&app, scope(s, s.analysis1))
            .await
            .unwrap();
        assert!(!pkg.governance.accepted_proposal_means_executed);
        assert!(!pkg.governance.human_review_mutates_original_claim);
        assert!(!pkg.governance.uncertainty_event_deletes_origin);
        assert!(!pkg.governance.context_presence_means_analysis_use);
        assert!(!pkg.governance.link_count_means_independent_evidence_count);
        assert!(!pkg.governance.system_auto_prioritizes_human_attention);
    }
}
