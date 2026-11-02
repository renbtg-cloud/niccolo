use niccolo_evidence::dag_evolution::experiment;
use std::fs;
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut reps = 15usize;
    let mut processes = 8usize;
    let args: Vec<_> = std::env::args().skip(1).collect();
    let (chunks, remainder) = args.as_chunks::<2>();
    if !remainder.is_empty() {
        return Err("use --reps N --processes N".into());
    }
    for pair in chunks {
        match pair[0].as_str() {
            "--reps" => reps = pair[1].parse()?,
            "--processes" => processes = pair[1].parse()?,
            x => return Err(format!("unknown option {x}").into()),
        }
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let reports = experiment(&root.join("target/dag-v004"), processes, reps)?;
    let mut out = String::from(
        "rule\ttrain_failed\ttrain_total\tholdout_failed\tholdout_total\tmedian_us\tp95_us\tconcurrent_failed\tconcurrent_total\tstatus\n",
    );
    let mut candidates = 0;
    for report in &reports {
        let status = if report.eligible_for_review {
            candidates += 1;
            "REVIEW_ONLY"
        } else {
            "REJECTED"
        };
        println!(
            "{}: failures train {}/{}, holdout {}/{}, concurrent {} -> {}",
            report.rule.label(),
            report.train.failures.len(),
            report.train.total,
            report.holdout.failures.len(),
            report.holdout.total,
            report
                .concurrency_failed
                .map_or("NOT_RUN".into(), |v| format!("{v}/{}", report.processes)),
            status
        );
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            report.rule.label(),
            report.train.failures.len(),
            report.train.total,
            report.holdout.failures.len(),
            report.holdout.total,
            report.median_us.map_or("NA".into(), |v| v.to_string()),
            report.p95_us.map_or("NA".into(), |v| v.to_string()),
            report
                .concurrency_failed
                .map_or("NOT_RUN".into(), |v| v.to_string()),
            if report.concurrency_failed.is_some() {
                report.processes
            } else {
                0
            },
            status
        ));
    }
    fs::create_dir_all(root.join("reports"))?;
    fs::write(root.join("reports/dag-evolution-v004.tsv"), out)?;
    println!(
        "{} rule configurations explored; {} eligible FOR REVIEW, none deployed",
        reports.len(),
        candidates
    );
    Ok(())
}
