//! A bounded, deterministic presentation of a *published* PostgreSQL analysis.
//!
//! Not a database backup, Git publisher, model-reasoning replay or source export.
//! The PostgreSQL analysis bundle (loaded with workspace authorization) is the
//! only source of values. CSV is presentation-friendly; JSON is lossless.
use crate::analysis_manifest::{self, AnalysisBundle, ManifestError};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::BTreeMap;
use uuid::Uuid;

const MAX_PACKAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_SOURCE_EXTRACTS: i64 = 128;
const MAX_SOURCE_EXTRACT_BYTES: usize = 512 * 1024;
const INSPECT_PY: &str = include_str!("../assets/presentation/inspect.py");

#[derive(Debug, thiserror::Error)]
pub enum PresentationError {
    #[error("published analysis unavailable: {0}")]
    Manifest(#[from] ManifestError),
    #[error("presentation serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("presentation exceeds the configured byte budget")]
    TooLarge,
    #[error("examined source extract selection exceeds the configured export budget")]
    SourceExtractTooLarge,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExaminedSourceExtract {
    pub document_id: Uuid,
    pub evidence_id: Uuid,
    pub ordinal: i32,
    pub format: String,
    pub locator: String,
    pub speaker_label: String,
    pub content: String,
    pub source_class: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresentationPackage {
    pub format: String,
    pub format_version: u32,
    pub analysis_id: Uuid,
    pub input_revision: i64,
    /// Paths are relative to the root of a presentation *directory*.
    /// They can later be written as files or committed to a user-owned Git repo.
    pub files: BTreeMap<String, String>,
    pub authoritative_state_remains_in_postgresql: bool,
    pub is_full_database_backup: bool,
    pub includes_original_source_fragment_contents: bool,
    pub includes_examined_source_fragment_contents: bool,
    pub examined_source_extracts_are_original_uploads: bool,
    pub examined_source_extracts_establish_truth: bool,
    pub reproduces_model_reasoning: bool,
    pub contains_complete_investigation_event_log: bool,
    pub published_to_git: bool,
}

fn csv_cell(value: &str) -> String {
    // Some recipients open a CSV in spreadsheet programs. Do not allow
    // untrusted claim text to become a spreadsheet formula. The JSON
    // projection preserves the exact original value.
    let trimmed = value.trim_start_matches([' ', '\t', '\r', '\n']);
    let spreadsheet_command = trimmed
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '=' | '+' | '-' | '@'))
        || value
            .chars()
            .next()
            .is_some_and(|c| matches!(c, '\t' | '\r' | '\n'));
    let safe = if spreadsheet_command {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}

fn csv(header: &[&str], rows: Vec<Vec<String>>) -> String {
    let mut out = String::new();
    out.push_str(
        &header
            .iter()
            .map(|name| csv_cell(name))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push('\n');
    for row in rows {
        out.push_str(
            &row.iter()
                .map(|x| csv_cell(x))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    out
}

/// Produce a presentation only from a recorded, immutable analysis bundle.
/// No caller-controlled evidence roots, invented model steps or inferred truth.
fn from_bundle_and_extracts(
    bundle: &AnalysisBundle,
    examined_source_extracts: &[ExaminedSourceExtract],
) -> Result<PresentationPackage, PresentationError> {
    let mut files: BTreeMap<String, String> = BTreeMap::new();

    files.insert(
        "data/analysis.json".into(),
        serde_json::to_string_pretty(&json!({
            "format": "niccolo.presentation-analysis",
            "format_version": 1,
            "bundle": bundle,
            "result_is_recomputed": false,
            "model_reasoning_is_reproducible": false,
            "source_fragment_bodies_included": !examined_source_extracts.is_empty(),
            "source_fragment_selection": if examined_source_extracts.is_empty() { "NONE" } else { "RECORDED_EXAMINED_ONLY" }
        }))?,
    );
    files.insert(
        "data/claims.csv".into(),
        csv(
            &[
                "claim_id",
                "ordinal",
                "kind",
                "text",
                "qualification",
                "created_by",
            ],
            bundle
                .claims
                .iter()
                .map(|c| {
                    vec![
                        c.claim_id.to_string(),
                        c.ordinal.to_string(),
                        c.kind.clone(),
                        c.text.clone(),
                        c.qualification.clone().unwrap_or_default(),
                        c.created_by.to_string(),
                    ]
                })
                .collect(),
        ),
    );
    files.insert(
        "data/claim_fragments.csv".into(),
        csv(
            &["claim_id", "evidence_id", "relation"],
            bundle
                .claim_fragment_links
                .iter()
                .map(|link| {
                    vec![
                        link.claim_id.to_string(),
                        link.evidence_id.to_string(),
                        link.relation.clone(),
                    ]
                })
                .collect(),
        ),
    );
    files.insert(
        "data/claim_contributions.csv".into(),
        csv(
            &["claim_id", "contribution_id", "relation"],
            bundle
                .claim_contribution_links
                .iter()
                .map(|link| {
                    vec![
                        link.claim_id.to_string(),
                        link.contribution_id.to_string(),
                        link.relation.clone(),
                    ]
                })
                .collect(),
        ),
    );
    files.insert(
        "data/reviews.csv".into(),
        csv(
            &["claim_id", "review_id", "kind", "note", "reviewer"],
            bundle
                .claim_reviews
                .iter()
                .map(|review| {
                    vec![
                        review.claim_id.to_string(),
                        review.review_id.to_string(),
                        review.kind.clone(),
                        review.note.clone(),
                        review.reviewer.to_string(),
                    ]
                })
                .collect(),
        ),
    );
    files.insert(
        "data/clarifications.csv".into(),
        csv(
            &["question_id", "reason", "proposed_by"],
            bundle
                .clarification_questions
                .iter()
                .map(|q| {
                    vec![
                        q.question_id.to_string(),
                        q.reason.clone(),
                        q.proposed_by.to_string(),
                    ]
                })
                .collect(),
        ),
    );
    files.insert(
        "data/answers.csv".into(),
        csv(
            &[
                "question_id",
                "answer_id",
                "contribution_id",
                "answered_by",
                "answer_revision",
            ],
            bundle
                .clarification_answers
                .iter()
                .map(|answer| {
                    vec![
                        answer.question_id.to_string(),
                        answer.answer_id.to_string(),
                        answer.contribution_id.to_string(),
                        answer.answered_by.to_string(),
                        answer.answer_revision.to_string(),
                    ]
                })
                .collect(),
        ),
    );
    files.insert(
        "data/examined_fragments.csv".into(),
        csv(
            &["evidence_id"],
            bundle
                .examined_fragments
                .iter()
                .map(|id| vec![id.to_string()])
                .collect(),
        ),
    );
    files.insert(
        "data/used_contributions.csv".into(),
        csv(
            &["contribution_id"],
            bundle
                .used_contributions
                .iter()
                .map(|id| vec![id.to_string()])
                .collect(),
        ),
    );
    files.insert(
        "data/claim_lineage.csv".into(),
        csv(
            &[
                "lineage_id",
                "prior_analysis_id",
                "prior_claim_id",
                "new_claim_id",
                "relation",
                "rationale",
            ],
            bundle
                .incoming_claim_lineage
                .iter()
                .map(|link| {
                    vec![
                        link.lineage_id.to_string(),
                        link.prior_analysis_id.to_string(),
                        link.prior_claim_id.to_string(),
                        link.new_claim_id.map(|x| x.to_string()).unwrap_or_default(),
                        link.relation.clone(),
                        link.rationale.clone(),
                    ]
                })
                .collect(),
        ),
    );
    files.insert(
        "data/clarification_impacts.csv".into(),
        csv(
            &[
                "impact_id",
                "source_analysis_id",
                "question_id",
                "answer_id",
                "contribution_id",
                "target_claim_id",
                "recorded_effect",
                "note",
            ],
            bundle
                .clarification_impacts
                .iter()
                .map(|i| {
                    vec![
                        i.impact_id.to_string(),
                        i.source_analysis_id.to_string(),
                        i.question_id.to_string(),
                        i.answer_id.to_string(),
                        i.contribution_id.to_string(),
                        i.target_claim_id.to_string(),
                        i.recorded_effect.clone(),
                        i.note.clone(),
                    ]
                })
                .collect(),
        ),
    );
    if !examined_source_extracts.is_empty() {
        files.insert(
            "data/examined_source_fragments.json".into(),
            serde_json::to_string_pretty(examined_source_extracts)?,
        );
        files.insert(
            "data/examined_source_fragments.csv".into(),
            csv(
                &[
                    "document_id",
                    "evidence_id",
                    "ordinal",
                    "format",
                    "locator",
                    "speaker_label",
                    "source_class",
                    "content",
                ],
                examined_source_extracts
                    .iter()
                    .map(|x| {
                        vec![
                            x.document_id.to_string(),
                            x.evidence_id.to_string(),
                            x.ordinal.to_string(),
                            x.format.clone(),
                            x.locator.clone(),
                            x.speaker_label.clone(),
                            x.source_class.clone(),
                            x.content.clone(),
                        ]
                    })
                    .collect(),
            ),
        );
    }
    files.insert("inspect.py".into(), INSPECT_PY.into());
    files.insert(
        "README.md".into(),
        format!(
            "# Niccolo - published analysis presentation\n\n\
             Analysis: {}\n\n\
             Input revision: {}\n\n\
             ## How to read it\n\n\
             Run: python3 inspect.py (requires only Python standard library).\n\n\
             The program checks the SHA-256 file list and presents the stored \
             question, published answer, coverage note, and explicit claim links. \
             Counts of links are NOT support weights, verified witnesses, calibrated \
             probabilities or proof of a claim.\n\n\
             ## Export scope and omissions\n\n\
             The JSON file contains the exact selected *published analysis bundle*. \
             CSV is a spreadsheet-safe presentation projection and may prefix \
             untrusted cell text with an apostrophe to prevent formula execution. \
             {} \
             Original uploaded document bytes are NOT included. The model's full \
             reasoning, intermediate steps, actual numeric source weights, all \
             historical events, and the full PostgreSQL state are NOT included. \
             Python does not reproduce the LLM reasoning or rerun the investigation.\n\n\
             All exported material may be sensitive. Share/publish it only \
             after explicit authorization and inspection. PostgreSQL remains \
             the operational source of truth. Git publication is a separate \
             feature, not performed by this package builder.\n\n\
             The SHA-256 list detects accidental file changes; it is not a \
             digital signature and does not authenticate a publisher.\n",
            bundle.analysis_id,
            bundle.input_revision,
            if examined_source_extracts.is_empty() {
                "Source fragment *IDs*, not source fragment contents, are included."
            } else {
                "This explicitly authorized variant includes only source fragment text recorded as examined by this analysis. Those excerpts are not original uploaded files, do not establish truth, and may contain sensitive material."
            }
        ),
    );

    if files
        .values()
        .any(|contents| contents.len() > MAX_PACKAGE_BYTES)
        || files.values().map(String::len).sum::<usize>() > MAX_PACKAGE_BYTES
    {
        return Err(PresentationError::TooLarge);
    }

    let sums: BTreeMap<String, String> = files
        .iter()
        .map(|(path, content)| {
            (
                path.clone(),
                format!("{:x}", Sha256::digest(content.as_bytes())),
            )
        })
        .collect();
    files.insert(
        "SHA256SUMS.json".into(),
        serde_json::to_string_pretty(&sums)?,
    );

    Ok(PresentationPackage {
        format: "niccolo.presentation-package".into(),
        format_version: 1,
        analysis_id: bundle.analysis_id,
        input_revision: bundle.input_revision,
        files,
        authoritative_state_remains_in_postgresql: true,
        is_full_database_backup: false,
        includes_original_source_fragment_contents: false,
        includes_examined_source_fragment_contents: !examined_source_extracts.is_empty(),
        examined_source_extracts_are_original_uploads: false,
        examined_source_extracts_establish_truth: false,
        reproduces_model_reasoning: false,
        contains_complete_investigation_event_log: false,
        published_to_git: false,
    })
}

pub fn from_bundle(bundle: &AnalysisBundle) -> Result<PresentationPackage, PresentationError> {
    from_bundle_and_extracts(bundle, &[])
}

async fn scoped(
    pool: &PgPool,
    workspace: Uuid,
) -> Result<Transaction<'_, Postgres>, PresentationError> {
    let mut tx = pool.begin().await.map_err(ManifestError::from)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(ManifestError::from)?;
    let privileged: bool = sqlx::query_scalar(
        "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname=current_user",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(ManifestError::from)?;
    if privileged {
        return Err(PresentationError::Manifest(ManifestError::Privileged));
    }
    let _: String = sqlx::query_scalar("SELECT set_config('niccolo.workspace_id',$1,true)")
        .bind(workspace.to_string())
        .fetch_one(&mut *tx)
        .await
        .map_err(ManifestError::from)?;
    crate::authz::bind_auth_digest(&mut tx)
        .await
        .map_err(ManifestError::from)?;
    Ok(tx)
}

async fn examined_source_extracts(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<Vec<ExaminedSourceExtract>, PresentationError> {
    let mut tx = scoped(pool, workspace).await?;
    type Row = (Uuid, Uuid, i32, String, String, String, String, String);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT d.document_id,f.evidence_id,f.ordinal,d.format,d.locator,
                f.speaker_label,e.content,e.source_class
           FROM niccolo.situation_analysis_snapshots a
           JOIN niccolo.situation_analysis_examined_fragments x
             ON x.workspace_id=a.workspace_id
            AND x.situation_id=a.situation_id
            AND x.analysis_id=a.analysis_id
           JOIN niccolo.source_fragments f
             ON f.workspace_id=x.workspace_id AND f.evidence_id=x.evidence_id
           JOIN niccolo.source_documents d
             ON d.workspace_id=f.workspace_id AND d.document_id=f.document_id
           JOIN niccolo.evidence e
             ON e.workspace_id=f.workspace_id AND e.evidence_id=f.evidence_id
           JOIN niccolo.situation_revision_sources rs
             ON rs.workspace_id=a.workspace_id
            AND rs.situation_id=a.situation_id
            AND rs.revision=a.input_revision
            AND rs.document_id=d.document_id
          WHERE a.workspace_id=$1 AND a.situation_id=$2 AND a.analysis_id=$3
          ORDER BY d.document_id,f.ordinal
          LIMIT $4",
    )
    .bind(workspace)
    .bind(situation)
    .bind(analysis)
    .bind(MAX_SOURCE_EXTRACTS + 1)
    .fetch_all(&mut *tx)
    .await
    .map_err(ManifestError::from)?;
    tx.commit().await.map_err(ManifestError::from)?;

    if rows.len() as i64 > MAX_SOURCE_EXTRACTS {
        return Err(PresentationError::SourceExtractTooLarge);
    }
    let mut bytes = 0usize;
    let mut out = Vec::with_capacity(rows.len());
    for (document_id, evidence_id, ordinal, format, locator, speaker_label, content, source_class) in rows {
        bytes = bytes.saturating_add(content.len());
        if bytes > MAX_SOURCE_EXTRACT_BYTES {
            return Err(PresentationError::SourceExtractTooLarge);
        }
        out.push(ExaminedSourceExtract {
            document_id,
            evidence_id,
            ordinal,
            format,
            locator,
            speaker_label,
            content,
            source_class,
        });
    }
    Ok(out)
}

pub async fn build(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<PresentationPackage, PresentationError> {
    // This load performs the authorized workspace-scoped PostgreSQL read.
    let bundle = analysis_manifest::load_bundle(pool, workspace, situation, analysis).await?;
    from_bundle(&bundle)
}

pub async fn build_with_examined_sources(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    analysis: Uuid,
) -> Result<PresentationPackage, PresentationError> {
    let bundle = analysis_manifest::load_bundle(pool, workspace, situation, analysis).await?;
    let extracts = examined_source_extracts(pool, workspace, situation, analysis).await?;
    from_bundle_and_extracts(&bundle, &extracts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis_manifest::{BundleClaim, BundleFragmentLink};

    fn example() -> AnalysisBundle {
        AnalysisBundle {
            bundle_version: 1,
            workspace_id: Uuid::from_u128(1),
            situation_id: Uuid::from_u128(2),
            analysis_id: Uuid::from_u128(3),
            input_revision: 7,
            input_fingerprint: "sha-256-example".into(),
            identity_event_no_max: 0,
            actor_kind_event_no_max: 0,
            actor_binding_event_no_max: 0,
            goal: "Determine what the sources actually support".into(),
            requested_by: Uuid::from_u128(4),
            reanalysis_of: None,
            answer_text: "Possible interpretation, not established fact".into(),
            coverage_note: "Only one source examined; budget constrained".into(),
            published_by: Uuid::from_u128(4),
            examined_fragments: vec![Uuid::from_u128(5)],
            used_contributions: vec![],
            clarification_questions: vec![],
            clarification_answers: vec![],
            claims: vec![BundleClaim {
                claim_id: Uuid::from_u128(6),
                ordinal: 1,
                kind: "HYPOTHESIS".into(),
                text: "One source reports a change".into(),
                qualification: Some("unverified".into()),
                created_by: Uuid::from_u128(4),
            }],
            claim_fragment_links: vec![BundleFragmentLink {
                claim_id: Uuid::from_u128(6),
                evidence_id: Uuid::from_u128(5),
                relation: "SUPPORTS".into(),
            }],
            claim_contribution_links: vec![],
            claim_reviews: vec![],
            incoming_claim_lineage: vec![],
            clarification_impacts: vec![],
        }
    }

    #[test]
    fn deterministic_package_only_contains_selected_analysis() {
        let a = from_bundle(&example()).unwrap();
        let b = from_bundle(&example()).unwrap();
        assert_eq!(a, b);
        assert!(a.files.contains_key("data/analysis.json"));
        assert!(a.files.contains_key("inspect.py"));
        assert!(a.files.contains_key("SHA256SUMS.json"));
        assert!(!a.is_full_database_backup);
        assert!(!a.published_to_git);
        assert!(!a.includes_original_source_fragment_contents);
        assert!(!a.includes_examined_source_fragment_contents);
        assert!(!a.examined_source_extracts_are_original_uploads);
        assert!(!a.examined_source_extracts_establish_truth);
        assert!(!a.reproduces_model_reasoning);
        assert!(a.authoritative_state_remains_in_postgresql);
    }

    #[test]
    fn explicitly_selected_examined_source_text_is_bounded_and_labeled_as_non_truth() {
        let source = ExaminedSourceExtract {
            document_id: Uuid::from_u128(10),
            evidence_id: Uuid::from_u128(5),
            ordinal: 1,
            format: "WHATSAPP_EXPORT".into(),
            locator: "chat.txt".into(),
            speaker_label: "Alice".into(),
            content: "=not a spreadsheet command, just untrusted source text".into(),
            source_class: "HUMAN_COMMUNICATION".into(),
        };
        let result = from_bundle_and_extracts(&example(), &[source]).unwrap();
        assert!(result.includes_examined_source_fragment_contents);
        assert!(!result.includes_original_source_fragment_contents);
        assert!(!result.examined_source_extracts_are_original_uploads);
        assert!(!result.examined_source_extracts_establish_truth);
        assert!(result.files.contains_key("data/examined_source_fragments.json"));
        assert!(result.files.contains_key("data/examined_source_fragments.csv"));
        assert!(result.files["README.md"].contains("explicitly authorized variant"));
        assert!(result.files["README.md"].contains("do not establish truth"));
        assert!(result.files["data/examined_source_fragments.csv"].contains("'=not a spreadsheet"));
    }

    #[test]
    fn csv_neutralizes_spreadsheet_formula_but_json_remains_lossless() {
        let mut input = example();
        input.claims[0].text = "=HYPERLINK(\"http://evil.example\", \"click\")\ntext".into();
        let result = from_bundle(&input).unwrap();
        assert!(
            result.files["data/claims.csv"]
                .contains("\"'=HYPERLINK(\"\"http://evil.example\"\", \"\"click\"\")\ntext\"")
        );
        assert!(result.files["data/analysis.json"].contains("=HYPERLINK"));
    }

    #[test]
    fn all_manifest_digests_match_payload_bytes() {
        let export = from_bundle(&example()).unwrap();
        let sums: BTreeMap<String, String> =
            serde_json::from_str(&export.files["SHA256SUMS.json"]).unwrap();
        assert_eq!(sums.len(), export.files.len() - 1);
        for (path, digest) in sums {
            assert_eq!(
                digest,
                format!("{:x}", Sha256::digest(export.files[&path].as_bytes()))
            );
        }
    }

    #[test]
    fn overlarge_presentations_fail_closed() {
        let mut input = example();
        input.answer_text = "z".repeat(MAX_PACKAGE_BYTES);
        assert!(matches!(
            from_bundle(&input),
            Err(PresentationError::TooLarge)
        ));
    }

    #[test]
    fn unknown_evidence_is_not_recast_as_independent_witness() {
        let export = from_bundle(&example()).unwrap();
        assert!(export.files["README.md"].contains("NOT support weights"));
        assert!(export.files["inspect.py"].contains("NOT evidence weights"));
    }
}
