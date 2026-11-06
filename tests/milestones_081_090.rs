use niccolo_evidence::analysis_claims::{self, NewClaim};
use niccolo_evidence::analysis_structure::AnalysisScope;
use niccolo_evidence::reasoning_handoff;
use niccolo_evidence::review_workflow::{self, ReviewSessionScope, SessionNote, UseRationale};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Seed {
    workspace: Uuid,
    situation: Uuid,
    principal: Uuid,
    analysis: Uuid,
    claim: Uuid,
    used_context: Uuid,
    unused_context: Uuid,
    examined_evidence: Uuid,
    unexamined_evidence: Uuid,
    question: Uuid,
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

async fn seed(admin: &PgPool, app: &PgPool) -> Seed {
    let s = Seed {
        workspace: Uuid::new_v4(),
        situation: Uuid::new_v4(),
        principal: Uuid::new_v4(),
        analysis: Uuid::new_v4(),
        claim: Uuid::new_v4(),
        used_context: Uuid::new_v4(),
        unused_context: Uuid::new_v4(),
        examined_evidence: Uuid::new_v4(),
        unexamined_evidence: Uuid::new_v4(),
        question: Uuid::new_v4(),
    };
    sqlx::query("INSERT INTO niccolo_auth.principals(principal_id) VALUES($1)")
        .bind(s.principal)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.workspaces(workspace_id,label) VALUES($1,'w')")
        .bind(s.workspace)
        .execute(admin)
        .await
        .unwrap();
    sqlx::query("INSERT INTO niccolo.situations(workspace_id,situation_id,focus,created_by_principal_id,current_revision) VALUES($1,$2,'focus',$3,1)")
        .bind(s.workspace).bind(s.situation).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revisions(workspace_id,situation_id,revision,focus,fingerprint,actor_principal_id,cause) VALUES($1,$2,1,'focus',$3,$4,'CREATED')")
        .bind(s.workspace).bind(s.situation).bind("a".repeat(64)).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_snapshots(workspace_id,situation_id,analysis_id,input_revision,input_fingerprint,goal,requested_by_principal_id) VALUES($1,$2,$3,1,$4,'understand what matters',$5)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind("a".repeat(64)).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_outputs(workspace_id,situation_id,analysis_id,answer_text,coverage_note,published_by_principal_id) VALUES($1,$2,$3,'alpha beta','bounded',$4)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_contributions(workspace_id,situation_id,contribution_id,author_principal_id,kind,contribution_text,status) VALUES($1,$2,$3,$5,'USER_REPORTED','used context','ACTIVE'),($1,$2,$4,$5,'USER_REPORTED','unused context','ACTIVE')")
        .bind(s.workspace).bind(s.situation).bind(s.used_context).bind(s.unused_context).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revision_contributions(workspace_id,situation_id,revision,contribution_id) VALUES($1,$2,1,$3),($1,$2,1,$4)")
        .bind(s.workspace).bind(s.situation).bind(s.used_context).bind(s.unused_context).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_used_contributions(workspace_id,situation_id,analysis_id,input_revision,contribution_id,recorded_by_principal_id) VALUES($1,$2,$3,1,$4,$5)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(s.used_context).bind(s.principal).execute(admin).await.unwrap();
    let d1 = Uuid::new_v4();
    let d2 = Uuid::new_v4();
    sqlx::query("INSERT INTO niccolo.evidence(workspace_id,evidence_id,source_locator,content,source_class) VALUES($1,$2,'d1:0-10','examined','DIRECT_OBSERVATION'),($1,$3,'d2:0-10','unexamined','DIRECT_OBSERVATION')")
        .bind(s.workspace).bind(s.examined_evidence).bind(s.unexamined_evidence).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_documents(workspace_id,document_id,format,locator,sha256,byte_length) VALUES($1,$2,'EML','d1',$4,100),($1,$3,'EML','d2',$5,100)")
        .bind(s.workspace).bind(d1).bind(d2).bind("3".repeat(64)).bind("4".repeat(64)).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.source_fragments(workspace_id,document_id,ordinal,evidence_id,byte_start,byte_end,speaker_label) VALUES($1,$2,0,$4,0,10,'Alice'),($1,$3,0,$5,0,10,'Bob')")
        .bind(s.workspace).bind(d1).bind(d2).bind(s.examined_evidence).bind(s.unexamined_evidence).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_sources(workspace_id,situation_id,document_id,added_by_principal_id) VALUES($1,$2,$3,$5),($1,$2,$4,$5)")
        .bind(s.workspace).bind(s.situation).bind(d1).bind(d2).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_revision_sources(workspace_id,situation_id,revision,document_id) VALUES($1,$2,1,$3),($1,$2,1,$4)")
        .bind(s.workspace).bind(s.situation).bind(d1).bind(d2).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_examined_fragments(workspace_id,situation_id,analysis_id,evidence_id,recorded_by_principal_id) VALUES($1,$2,$3,$4,$5)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(s.examined_evidence).bind(s.principal).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.investigative_questions(workspace_id,question_id,question) VALUES($1,$2,'What would clarify?')")
        .bind(s.workspace).bind(s.question).execute(admin).await.unwrap();
    sqlx::query("INSERT INTO niccolo.situation_analysis_clarification_questions(workspace_id,situation_id,analysis_id,question_id,reason,proposed_by_principal_id) VALUES($1,$2,$3,$4,'would clarify',$5)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(s.question).bind(s.principal).execute(admin).await.unwrap();
    analysis_claims::record_claim(
        app,
        s.workspace,
        s.situation,
        s.analysis,
        s.principal,
        NewClaim {
            claim_id: s.claim,
            ordinal: 1,
            kind: "INFERENCE".into(),
            text: "alpha".into(),
            qualification: Some("provisional".into()),
        },
    )
    .await
    .unwrap();
    s
}

fn analysis_scope(s: Seed) -> AnalysisScope {
    AnalysisScope {
        workspace: s.workspace,
        situation: s.situation,
        analysis: s.analysis,
    }
}

#[tokio::test]
async fn v081_review_session_has_explicit_start_and_single_append_only_end() {
    let (admin, app) = pools().await;
    let s = seed(&admin, &app).await;
    let session = Uuid::new_v4();
    review_workflow::start_session(
        &app,
        s.workspace,
        s.situation,
        session,
        Some(s.analysis),
        "review claims",
        s.principal,
    )
    .await
    .unwrap();
    let scope = ReviewSessionScope {
        workspace: s.workspace,
        situation: s.situation,
        session,
    };
    let before = review_workflow::session_digest(&app, scope).await.unwrap();
    assert!(!before.ended);
    assert!(!before.session_means_analysis_approved);
    review_workflow::end_session(
        &app,
        scope,
        Uuid::new_v4(),
        Some("done reviewing"),
        s.principal,
    )
    .await
    .unwrap();
    assert!(
        review_workflow::session_digest(&app, scope)
            .await
            .unwrap()
            .ended
    );
    assert!(
        review_workflow::end_session(&app, scope, Uuid::new_v4(), None, s.principal)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn v082_v083_notes_bookmarks_and_digest_are_human_owned() {
    let (admin, app) = pools().await;
    let s = seed(&admin, &app).await;
    let session = Uuid::new_v4();
    review_workflow::start_session(
        &app,
        s.workspace,
        s.situation,
        session,
        Some(s.analysis),
        "inspect",
        s.principal,
    )
    .await
    .unwrap();
    let scope = ReviewSessionScope {
        workspace: s.workspace,
        situation: s.situation,
        session,
    };
    review_workflow::add_session_note(
        &app,
        scope,
        &SessionNote {
            note_id: Uuid::new_v4(),
            kind: "NOTE".into(),
            target_type: None,
            target_id: None,
            text: "my note".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    review_workflow::add_session_note(
        &app,
        scope,
        &SessionNote {
            note_id: Uuid::new_v4(),
            kind: "BOOKMARK".into(),
            target_type: Some("CLAIM".into()),
            target_id: Some(s.claim),
            text: "return here".into(),
            recorded_by: s.principal,
        },
    )
    .await
    .unwrap();
    let d = review_workflow::session_digest(&app, scope).await.unwrap();
    assert_eq!((d.note_count, d.bookmark_count), (1, 1));
    assert!(!d.session_means_analysis_approved);
}

#[tokio::test]
async fn v084_context_use_rationale_requires_context_actually_used() {
    let (admin, app) = pools().await;
    let s = seed(&admin, &app).await;
    let item = UseRationale {
        rationale_id: Uuid::new_v4(),
        reason_kind: "GOAL_RELEVANCE".into(),
        reason: "directly changes the interpretation".into(),
        recorded_by: s.principal,
    };
    review_workflow::record_context_use_rationale(&app, analysis_scope(s), s.used_context, &item)
        .await
        .unwrap();
    let bad = UseRationale {
        rationale_id: Uuid::new_v4(),
        ..item
    };
    assert!(
        review_workflow::record_context_use_rationale(
            &app,
            analysis_scope(s),
            s.unused_context,
            &bad
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn v085_fragment_rationale_requires_fragment_actually_examined() {
    let (admin, app) = pools().await;
    let s = seed(&admin, &app).await;
    let item = UseRationale {
        rationale_id: Uuid::new_v4(),
        reason_kind: "CROSS_CHECK".into(),
        reason: "cross-check wording".into(),
        recorded_by: s.principal,
    };
    review_workflow::record_fragment_rationale(&app, analysis_scope(s), s.examined_evidence, &item)
        .await
        .unwrap();
    let bad = UseRationale {
        rationale_id: Uuid::new_v4(),
        ..item
    };
    assert!(
        review_workflow::record_fragment_rationale(
            &app,
            analysis_scope(s),
            s.unexamined_evidence,
            &bad
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn v086_claim_questions_show_answer_state_without_rewriting_claim() {
    let (admin, app) = pools().await;
    let s = seed(&admin, &app).await;
    review_workflow::link_claim_question(
        &app,
        analysis_scope(s),
        s.claim,
        s.question,
        "WOULD_CLARIFY",
        s.principal,
    )
    .await
    .unwrap();
    let q = review_workflow::claim_questions(&app, analysis_scope(s))
        .await
        .unwrap();
    assert_eq!(q.len(), 1);
    assert!(!q[0].answered);
    sqlx::query("INSERT INTO niccolo.situation_analysis_clarification_answers(workspace_id,situation_id,analysis_id,question_id,answer_id,contribution_id,answered_by_principal_id,answer_revision) VALUES($1,$2,$3,$4,$5,$6,$7,1)")
        .bind(s.workspace).bind(s.situation).bind(s.analysis).bind(s.question).bind(Uuid::new_v4()).bind(s.unused_context).bind(s.principal).execute(&admin).await.unwrap();
    let q = review_workflow::claim_questions(&app, analysis_scope(s))
        .await
        .unwrap();
    assert!(q[0].answered);
    let claims = analysis_claims::list_claims(&app, s.workspace, s.situation, s.analysis)
        .await
        .unwrap();
    assert_eq!(claims[0].text, "alpha");
}

#[tokio::test]
async fn v087_v088_revision_condition_and_events_are_append_only_and_attributed() {
    let (admin, app) = pools().await;
    let s = seed(&admin, &app).await;
    let condition = Uuid::new_v4();
    review_workflow::record_revision_condition(
        &app,
        analysis_scope(s),
        s.claim,
        condition,
        "NEW_EVIDENCE",
        "a direct contrary observation would require revisiting this",
        s.principal,
    )
    .await
    .unwrap();
    review_workflow::append_condition_event(
        &app,
        analysis_scope(s),
        s.claim,
        condition,
        Uuid::new_v4(),
        "NOT_MET",
        "current material does not meet condition",
        None,
        None,
        s.principal,
    )
    .await
    .unwrap();
    review_workflow::append_condition_event(
        &app,
        analysis_scope(s),
        s.claim,
        condition,
        Uuid::new_v4(),
        "MET",
        "new fragment meets condition",
        Some(s.examined_evidence),
        None,
        s.principal,
    )
    .await
    .unwrap();
    let c = review_workflow::revision_conditions(&app, analysis_scope(s))
        .await
        .unwrap();
    assert_eq!(c[0].latest_event.as_deref(), Some("MET"));
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM niccolo.situation_analysis_claim_condition_events WHERE workspace_id=$1 AND situation_id=$2 AND analysis_id=$3 AND claim_id=$4 AND condition_id=$5").bind(s.workspace).bind(s.situation).bind(s.analysis).bind(s.claim).bind(condition).fetch_one(&admin).await.unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn v089_met_condition_only_suggests_reanalysis_and_reopen_removes_candidate() {
    let (admin, app) = pools().await;
    let s = seed(&admin, &app).await;
    let condition = Uuid::new_v4();
    review_workflow::record_revision_condition(
        &app,
        analysis_scope(s),
        s.claim,
        condition,
        "CLARIFICATION",
        "if the user clarifies timing, revisit",
        s.principal,
    )
    .await
    .unwrap();
    review_workflow::append_condition_event(
        &app,
        analysis_scope(s),
        s.claim,
        condition,
        Uuid::new_v4(),
        "MET",
        "clarification arrived",
        None,
        Some(s.used_context),
        s.principal,
    )
    .await
    .unwrap();
    let c = review_workflow::reanalysis_candidates(&app, analysis_scope(s))
        .await
        .unwrap();
    assert_eq!(c.len(), 1);
    assert!(c[0].suggestion_only);
    assert!(!c[0].analysis_started);
    review_workflow::append_condition_event(
        &app,
        analysis_scope(s),
        s.claim,
        condition,
        Uuid::new_v4(),
        "REOPENED",
        "human wants more evidence",
        None,
        None,
        s.principal,
    )
    .await
    .unwrap();
    assert!(
        review_workflow::reanalysis_candidates(&app, analysis_scope(s))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn v090_handoff_is_deterministic_and_grants_no_authority() {
    let (admin, app) = pools().await;
    let s = seed(&admin, &app).await;
    review_workflow::link_claim_question(
        &app,
        analysis_scope(s),
        s.claim,
        s.question,
        "OPEN_QUESTION",
        s.principal,
    )
    .await
    .unwrap();
    let condition = Uuid::new_v4();
    review_workflow::record_revision_condition(
        &app,
        analysis_scope(s),
        s.claim,
        condition,
        "OTHER",
        "another interpretation could change this",
        s.principal,
    )
    .await
    .unwrap();
    let a = reasoning_handoff::bundle(&app, analysis_scope(s))
        .await
        .unwrap();
    let b = reasoning_handoff::bundle(&app, analysis_scope(s))
        .await
        .unwrap();
    assert_eq!(
        reasoning_handoff::canonical_json(&a).unwrap(),
        reasoning_handoff::canonical_json(&b).unwrap()
    );
    assert_eq!(
        reasoning_handoff::sha256(&a).unwrap(),
        reasoning_handoff::sha256(&b).unwrap()
    );
    assert!(!a.handoff_grants_authority);
    assert!(!a.handoff_starts_analysis);
    assert!(!a.handoff_executes_actions);
    assert_eq!(a.open_questions.len(), 1);
    assert_eq!(a.revision_conditions.len(), 1);
}
