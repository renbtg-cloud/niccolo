use niccolo_evidence::benchmark::*;
use std::collections::BTreeSet;

fn main() {
    println!("Niccolò benchmark contract — OFFLINE SYNTHETIC POC");
    let case = RawCase {
        case_id: "synthetic-borgia".into(),
        question: "Do three apparent witnesses establish an independent consensus?".into(),
        original_records: vec![
            OriginalRecord {
                source_id: "memo".into(),
                text: "Clerk wrote one memo: blue-thread packet".into(),
                original_headers: vec![],
            },
            OriginalRecord {
                source_id: "report-a".into(),
                text: "Witness A quotes blue-thread packet".into(),
                original_headers: vec![],
            },
            OriginalRecord {
                source_id: "report-b".into(),
                text: "Witness B quotes blue-thread packet".into(),
                original_headers: vec![],
            },
            OriginalRecord {
                source_id: "report-c".into(),
                text: "Witness C quotes blue-thread packet".into(),
                original_headers: vec![],
            },
        ],
    };
    let request =
        prepare_blind_frontier_request(&case, AgentResources::competent_frontier_baseline())
            .unwrap();
    println!(
        "Blind frontier input: {} original records, {} investigation steps available, {} tools",
        request.original_records.len(),
        request.resources.max_agent_steps,
        request.resources.allowed_tools.len()
    );
    let policy = EvaluationCostPolicy {
        background_evaluations_enabled: true,
        extra_model_probes_allowed: true,
        permitted_providers: BTreeSet::from(["demo-authorized-provider".into()]),
        prohibited_models: BTreeSet::new(),
        monthly_cap_micro_usd: Some(10_000_000),
        monthly_spent_micro_usd: 1_000_000,
        monthly_reserved_micro_usd: 500_000,
        maximum_background_utilization_per_mille: 300,
    };
    let check = authorize_opportunistic_frontier_probe(
        &policy,
        &ProposedFrontierProbe {
            provider: "demo-authorized-provider",
            model: "frontier-demo",
            original_data_egress_authorized: true,
            estimated_incremental_micro_usd: 100_000,
        },
    );
    println!("Sample evaluation permission gate: {check:?}");

    use niccolo_evidence::{Evidence, EvidenceClass, EvidenceId, EvidenceIndex};
    let mut evidence = EvidenceIndex::default();
    evidence
        .insert(Evidence::new(
            1,
            None,
            EvidenceClass::ReportedSpeech,
            "one clerk memo",
        ))
        .unwrap();
    for id in 2..=4 {
        evidence
            .insert(Evidence::new(
                id,
                Some(1),
                EvidenceClass::CopyOrTranscript,
                "copied claim",
            ))
            .unwrap();
    }
    let naive_witness_count = 3;
    let traced = evidence
        .assess_lineages(&[EvidenceId(2), EvidenceId(3), EvidenceId(4)])
        .unwrap();
    println!(
        "Algorithmic reproducible counterexample: naive reports={naive_witness_count}, traceable transmission roots={}, independence established={}",
        traced.traceable_roots.len(),
        traced.independence_established
    );
    println!("No real frontier call or frontier quality comparison was performed.");
}
