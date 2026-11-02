//! Domain-limited, reproducible, offline Rust candidate evolution experiments.
//! Deliberately does not execute arbitrary third-party code or deploy anything.
//! 'Self-evolution' here means selecting among generated, trusted implementations.
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub const TRUSTED_TEMPLATE: &str = include_str!("../templates/lineage_candidate.rs");
pub const TRAIN_FIXTURES: &str = include_str!("../fixtures/train.txt");
pub const HOLDOUT_FIXTURES: &str = include_str!("../fixtures/holdout.txt");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub id: String,
    pub edges: String,
    pub observations: String,
    pub expected: String,
}

pub fn parse_fixtures(raw: &str) -> Result<Vec<Case>, String> {
    let mut ids = BTreeSet::new();
    let mut cases = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<_> = line.split('|').collect();
        if parts.len() != 4 || parts.iter().any(|p| p.is_empty()) {
            return Err(format!("invalid fixture at line {}", i + 1));
        }
        if !ids.insert(parts[0].to_owned()) {
            return Err(format!("duplicate case {}", parts[0]));
        }
        if parts[3] != "UNKNOWN" && !parts[3].starts_with("COUNT:") {
            return Err(format!("invalid oracle at line {}", i + 1));
        }
        cases.push(Case {
            id: parts[0].into(),
            edges: parts[1].into(),
            observations: parts[2].into(),
            expected: parts[3].into(),
        });
    }
    if cases.is_empty() {
        return Err("empty fixture suite".into());
    }
    Ok(cases)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Naive = 0,
    OneStep = 1,
    TraceRoot = 2,
}

#[derive(Debug, Clone, Copy)]
pub struct Candidate {
    pub id: &'static str,
    pub strategy: Strategy,
}

pub const CANDIDATES: [Candidate; 3] = [
    Candidate {
        id: "naive_report_count",
        strategy: Strategy::Naive,
    },
    Candidate {
        id: "one_step_parent",
        strategy: Strategy::OneStep,
    },
    Candidate {
        id: "transitive_trace_root",
        strategy: Strategy::TraceRoot,
    },
];

/// Generates real Rust source from a curated, reviewed template. This is NOT a
/// general LLM-code execution sandbox and MUST NOT accept arbitrary Rust input.
pub fn generate(candidate: Candidate) -> String {
    TRUSTED_TEMPLATE.replace("__STRATEGY__", &(candidate.strategy as u8).to_string())
}

pub fn compile_candidate(candidate: Candidate, target: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(target).map_err(|e| e.to_string())?;
    let source = target.join(format!("{}.rs", candidate.id));
    let binary = target.join(candidate.id);
    fs::write(&source, generate(candidate)).map_err(|e| e.to_string())?;
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let output = Command::new(rustc)
        .arg("--edition=2024")
        .arg("-O")
        .arg("-C")
        .arg("debuginfo=0")
        .arg("--crate-name")
        .arg(candidate.id)
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|e| format!("rustc could not execute: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "candidate {} compilation failed: {}",
            candidate.id,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(binary)
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub total: usize,
    pub failures: Vec<String>,
    pub elapsed: Duration,
}
impl Outcome {
    pub fn passed(&self) -> usize {
        self.total - self.failures.len()
    }
    pub fn clean(&self) -> bool {
        self.failures.is_empty()
    }
}

fn candidate_input(cases: &[Case]) -> String {
    // IMPORTANT: oracle/expected outputs are intentionally excluded.
    let mut input = String::new();
    for case in cases {
        input.push_str(&format!(
            "{}|{}|{}\n",
            case.id, case.edges, case.observations
        ));
    }
    input
}

pub fn run_candidate(binary: &Path, cases: &[Case], deadline: Duration) -> Result<Outcome, String> {
    if cases.is_empty() {
        return Err("no cases".into());
    }
    let start = Instant::now();
    let mut child = Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start {}: {e}", binary.display()))?;
    let input = candidate_input(cases);
    let write_result = child
        .stdin
        .take()
        .ok_or("missing stdin")?
        .write_all(input.as_bytes());
    if let Err(e) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("candidate input failed: {e}"));
    }
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < deadline => thread::sleep(Duration::from_millis(1)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("candidate exceeded {:?} deadline", deadline));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    let elapsed = start.elapsed();
    if !output.status.success() {
        return Err(format!(
            "candidate crashed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let text = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    let answers: Vec<_> = text.lines().collect();
    let mut failures = Vec::new();
    if answers.len() != cases.len() {
        failures.push(format!(
            "output count {} != cases {}",
            answers.len(),
            cases.len()
        ));
    }
    for (i, case) in cases.iter().enumerate() {
        let expected = format!("{}|{}", case.id, case.expected);
        if answers.get(i).copied() != Some(expected.as_str()) {
            failures.push(format!(
                "{} expected {} got {:?}",
                case.id,
                expected,
                answers.get(i)
            ));
        }
    }
    Ok(Outcome {
        total: cases.len(),
        failures,
        elapsed,
    })
}

pub fn benchmark_candidate(
    binary: &Path,
    cases: &[Case],
    warmups: usize,
    repetitions: usize,
) -> Result<(u128, u128), String> {
    if repetitions == 0 {
        return Err("benchmark repetitions must be > 0".into());
    }
    for _ in 0..warmups {
        if !run_candidate(binary, cases, Duration::from_secs(3))?.clean() {
            return Err("quality regression during warmup".into());
        }
    }
    let mut samples = Vec::new();
    for _ in 0..repetitions {
        let outcome = run_candidate(binary, cases, Duration::from_secs(3))?;
        if !outcome.clean() {
            return Err("quality regression during benchmark".into());
        }
        samples.push(outcome.elapsed.as_micros());
    }
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    let p95_index = ((samples.len() * 95).div_ceil(100)).saturating_sub(1);
    Ok((median, samples[p95_index]))
}

pub fn concurrent_process_check(
    binary: &Path,
    cases: &[Case],
    independent_processes: usize,
) -> Result<(usize, usize), String> {
    if independent_processes == 0 || independent_processes > 64 {
        return Err("process count must be 1..=64".into());
    }
    let mut threads = Vec::new();
    for _ in 0..independent_processes {
        let b = binary.to_owned();
        let c = cases.to_vec();
        threads.push(thread::spawn(move || {
            run_candidate(&b, &c, Duration::from_secs(5))
        }));
    }
    let mut failed_processes = 0;
    let mut failed_cases = 0;
    for thread in threads {
        match thread.join() {
            Ok(Ok(outcome)) => {
                if !outcome.clean() {
                    failed_processes += 1;
                    failed_cases += outcome.failures.len();
                }
            }
            Ok(Err(_)) | Err(_) => {
                failed_processes += 1;
                failed_cases += cases.len();
            }
        }
    }
    Ok((failed_processes, failed_cases))
}

#[derive(Debug)]
pub struct CandidateReport {
    pub name: String,
    pub train: Outcome,
    pub holdout: Outcome,
    pub benchmark_median_us: Option<u128>,
    pub benchmark_p95_us: Option<u128>,
    pub concurrency_failed_processes: Option<usize>,
    pub concurrent_processes: usize,
    pub staged_for_review: bool,
}

pub fn evaluate_all(
    dir: &Path,
    processes: usize,
    reps: usize,
) -> Result<Vec<CandidateReport>, String> {
    if reps == 0 {
        return Err("reps must be nonzero".into());
    }
    let train = parse_fixtures(TRAIN_FIXTURES)?;
    let holdout = parse_fixtures(HOLDOUT_FIXTURES)?;
    if !train.iter().all(|t| holdout.iter().all(|h| h.id != t.id)) {
        return Err("training/holdout case overlap".into());
    }
    let mut reports = Vec::new();
    for candidate in CANDIDATES {
        let binary = compile_candidate(candidate, dir)?;
        let train_outcome = run_candidate(&binary, &train, Duration::from_secs(3))?;
        let holdout_outcome = run_candidate(&binary, &holdout, Duration::from_secs(3))?;
        let clean = train_outcome.clean() && holdout_outcome.clean();
        let (median, p95) = if clean {
            let (m, p) = benchmark_candidate(&binary, &holdout, 3, reps)?;
            (Some(m), Some(p))
        } else {
            (None, None)
        };
        let concurrency_failed = if clean {
            Some(concurrent_process_check(&binary, &holdout, processes)?.0)
        } else {
            None
        };
        reports.push(CandidateReport {
            name: candidate.id.into(),
            train: train_outcome,
            holdout: holdout_outcome,
            benchmark_median_us: median,
            benchmark_p95_us: p95,
            concurrency_failed_processes: concurrency_failed,
            concurrent_processes: processes,
            staged_for_review: clean && concurrency_failed == Some(0),
        });
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixtures_parsed_and_distinct() {
        let train = parse_fixtures(TRAIN_FIXTURES).unwrap();
        let holdout = parse_fixtures(HOLDOUT_FIXTURES).unwrap();
        assert_eq!(train.len(), 8);
        assert_eq!(holdout.len(), 16);
        for c in train {
            assert!(!holdout.iter().any(|h| h.id == c.id));
        }
    }
    #[test]
    fn rejects_duplicate_labels() {
        assert!(parse_fixtures("a|1:0|1|COUNT:1\na|1:0|1|COUNT:1").is_err());
    }
    #[test]
    fn rejects_invalid_oracle() {
        assert!(parse_fixtures("a|1:0|1|probably_true").is_err());
    }
    #[test]
    fn rejects_empty_suite() {
        assert!(parse_fixtures("# nothing").is_err());
    }
    #[test]
    fn candidate_source_contains_no_oracle_or_holdout() {
        for candidate in CANDIDATES {
            let source = generate(candidate);
            assert!(!source.contains("complex_shared_root"));
            assert!(!source.contains("COUNT:4\n"));
            assert!(!source.contains("__STRATEGY__"));
        }
    }
    #[test]
    fn candidate_input_strips_expected_results() {
        let fixture = parse_fixtures("x|1:0,2:1|2|COUNT:1").unwrap();
        assert_eq!(candidate_input(&fixture), "x|1:0,2:1|2\n");
    }
    #[test]
    fn rejects_zero_benchmark_repetitions() {
        assert!(benchmark_candidate(Path::new("/nonexistent"), &[], 0, 0).is_err());
    }
    #[test]
    fn rejects_invalid_process_count() {
        assert!(concurrent_process_check(Path::new("/nonexistent"), &[], 0).is_err());
        assert!(concurrent_process_check(Path::new("/nonexistent"), &[], 65).is_err());
    }
    #[test]
    fn template_compiler_generates_actual_executables() {
        let dir =
            std::env::temp_dir().join(format!("niccolo-evolution-test-{}", std::process::id()));
        let bin = compile_candidate(CANDIDATES[2], &dir).unwrap();
        let cases = parse_fixtures(HOLDOUT_FIXTURES).unwrap();
        let outcome = run_candidate(&bin, &cases, Duration::from_secs(3)).unwrap();
        assert!(outcome.clean(), "{:?}", outcome.failures);
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn naive_candidate_is_adversarially_refuted() {
        let dir = std::env::temp_dir().join(format!("niccolo-naive-test-{}", std::process::id()));
        let bin = compile_candidate(CANDIDATES[0], &dir).unwrap();
        let cases = parse_fixtures(HOLDOUT_FIXTURES).unwrap();
        let outcome = run_candidate(&bin, &cases, Duration::from_secs(3)).unwrap();
        assert!(!outcome.clean());
        let _ = fs::remove_dir_all(dir);
    }
}
