use niccolo_evidence::evolution::evaluate_all;
use std::fs;
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut processes = 4usize;
    let mut reps = 21usize;
    let args: Vec<String> = std::env::args().collect();
    for pair in args[1..].as_chunks::<2>().0 {
        match pair[0].as_str() {
            "--processes" => processes = pair[1].parse()?,
            "--reps" => reps = pair[1].parse()?,
            _ => return Err(format!("Unknown option {}", pair[0]).into()),
        }
    }
    if !(args.len() - 1).is_multiple_of(2) {
        return Err("Expected option/value pairs".into());
    }
    let build_dir = base.join("target/evolution-candidates");
    let reports = evaluate_all(&build_dir, processes, reps)?;
    let mut tsv = String::from(
        "candidate\ttrain_failed\ttrain_total\tholdout_failed\tholdout_total\tmedian_us\tp95_us\tconcurrent_failed\tconcurrent_total\tstatus\n",
    );
    for report in &reports {
        let status = if report.staged_for_review {
            "ELIGIBLE_FOR_REVIEW_NOT_DEPLOYED"
        } else {
            "REJECTED"
        };
        let concurrence = report
            .concurrency_failed_processes
            .map_or("NOT_RUN".to_owned(), |failed| {
                format!("{failed}/{}", report.concurrent_processes)
            });
        println!(
            "{}: training failures {}/{}, holdout failures {}/{}, concurrent process failures {}, median={}us p95={}us => {}",
            report.name,
            report.train.failures.len(),
            report.train.total,
            report.holdout.failures.len(),
            report.holdout.total,
            concurrence,
            report
                .benchmark_median_us
                .map_or("N/A".into(), |v| v.to_string()),
            report
                .benchmark_p95_us
                .map_or("N/A".into(), |v| v.to_string()),
            status
        );
        for failure in report.holdout.failures.iter().take(4) {
            println!("   FAILED: {failure}");
        }
        tsv.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            report.name,
            report.train.failures.len(),
            report.train.total,
            report.holdout.failures.len(),
            report.holdout.total,
            report
                .benchmark_median_us
                .map_or("N/A".into(), |v| v.to_string()),
            report
                .benchmark_p95_us
                .map_or("N/A".into(), |v| v.to_string()),
            report
                .concurrency_failed_processes
                .map_or("NOT_RUN".to_owned(), |v| v.to_string()),
            if report.concurrency_failed_processes.is_some() {
                report.concurrent_processes
            } else {
                0
            },
            status
        ));
    }
    let out = base.join("reports");
    fs::create_dir_all(&out)?;
    let path = out.join("evolution-v003.tsv");
    fs::write(&path, tsv)?;
    println!("REPORT={}", path.display());
    println!(
        "NOTE: toy trusted strategies only; NO LLM calls, NO arbitrary code, NO production promotion."
    );
    Ok(())
}
