use niccolo_evidence::{
    Evidence, EvidenceClass, EvidenceId, EvidenceIndex, Expression, propose_access_question,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let expr = Expression::says(
        "Jane",
        Expression::believes("Joe", Expression::event("Mary", "criticized", "James")),
    );
    println!("Attributed expression: {expr}");
    let mut e = EvidenceIndex::default();
    e.insert(Evidence::new(
        1,
        None,
        EvidenceClass::ReportedSpeech,
        "Clerk memorandum",
    ))
    .unwrap();
    for id in 2..=4 {
        e.insert(Evidence::new(
            id,
            Some(1),
            EvidenceClass::CopyOrTranscript,
            "A witness repeats the memorandum",
        ))
        .unwrap();
    }
    let ids = (2..=4).map(EvidenceId).collect::<Vec<_>>();
    let report = e.assess_lineages(&ids).unwrap();
    println!(
        "{} recorded testimonies; {} documented origin lineage; independence proven: {}",
        report.admitted_reports,
        report.traceable_roots.len(),
        report.independence_established
    );
    println!(
        "Investigative lead (NOT a finding): {}",
        propose_access_question(ids).question
    );
    println!(
        "No inference about authorship, guilt, motive or deliberate coordination has been established."
    );
    Ok(())
}
