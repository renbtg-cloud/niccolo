//! v0.4 constrained typed rule search. Zero external crates, no arbitrary uploads.
//! Rules select a strategy from a small auditable grammar, then compile actual Rust binaries.
use crate::evolution::{self, Case, Outcome};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub const TEMPLATE: &str = include_str!("../templates/dag_candidate.rs");
pub const TRAIN: &str = include_str!("../fixtures/dag-train.txt");
pub const HOLDOUT: &str = include_str!("../fixtures/dag-holdout.txt");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Traversal {
    None,
    OneHop,
    Full,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MissingPolicy {
    AssumeRoot,
    FailClosed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RevocationPolicy {
    Ignore,
    FailClosed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuleSpec {
    pub traversal: Traversal,
    pub missing: MissingPolicy,
    pub revoked: RevocationPolicy,
}
impl RuleSpec {
    pub fn label(self) -> String {
        format!(
            "depth{}_miss{}_rev{}",
            self.traversal as u8, self.missing as u8, self.revoked as u8
        )
    }
    pub fn source(self) -> String {
        TEMPLATE
            .replace("__DEPTH__", &(self.traversal as u8).to_string())
            .replace(
                "__MISSING__",
                match self.missing {
                    MissingPolicy::AssumeRoot => "false",
                    MissingPolicy::FailClosed => "true",
                },
            )
            .replace(
                "__REVOKED__",
                match self.revoked {
                    RevocationPolicy::Ignore => "false",
                    RevocationPolicy::FailClosed => "true",
                },
            )
    }
}

/// Exhaustive enumeration of a constrained reasoning language, not LLM synthesis.
pub fn propose_rules() -> Vec<RuleSpec> {
    let mut rules = Vec::new();
    for traversal in [Traversal::None, Traversal::OneHop, Traversal::Full] {
        for missing in [MissingPolicy::AssumeRoot, MissingPolicy::FailClosed] {
            for revoked in [RevocationPolicy::Ignore, RevocationPolicy::FailClosed] {
                rules.push(RuleSpec {
                    traversal,
                    missing,
                    revoked,
                });
            }
        }
    }
    rules
}

pub fn compile(rule: RuleSpec, dir: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let src = dir.join(format!("{}.rs", rule.label()));
    let bin = dir.join(rule.label());
    fs::write(&src, rule.source()).map_err(|e| e.to_string())?;
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let output = Command::new(rustc)
        .arg("--edition=2024")
        .arg("-O")
        .arg("--crate-name")
        .arg(rule.label())
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .map_err(|e| format!("compiler unavailable: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed to compile: {}",
            rule.label(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(bin)
}

pub fn verify_suite_disjoint(train: &[Case], holdout: &[Case]) -> Result<(), String> {
    let names: BTreeSet<_> = train.iter().map(|c| c.id.as_str()).collect();
    let shapes: BTreeSet<_> = train.iter().map(|c| (&c.edges, &c.observations)).collect();
    if holdout
        .iter()
        .any(|h| names.contains(h.id.as_str()) || shapes.contains(&(&h.edges, &h.observations)))
    {
        return Err("duplicate id or exact input between training and holdout".into());
    }
    Ok(())
}

#[derive(Debug)]
pub struct RuleResult {
    pub rule: RuleSpec,
    pub train: Outcome,
    pub holdout: Outcome,
    pub median_us: Option<u128>,
    pub p95_us: Option<u128>,
    pub concurrency_failed: Option<usize>,
    pub processes: usize,
    pub eligible_for_review: bool,
}

pub fn experiment(dir: &Path, processes: usize, reps: usize) -> Result<Vec<RuleResult>, String> {
    if !(1..=64).contains(&processes) || !(1..=200).contains(&reps) {
        return Err("processes must be 1..64 and reps 1..200".into());
    }
    let train = evolution::parse_fixtures(TRAIN)?;
    let holdout = evolution::parse_fixtures(HOLDOUT)?;
    verify_suite_disjoint(&train, &holdout)?;
    let mut reports = Vec::new();
    for rule in propose_rules() {
        let bin = compile(rule, dir)?;
        let training = evolution::run_candidate(&bin, &train, Duration::from_secs(3))?;
        let testing = evolution::run_candidate(&bin, &holdout, Duration::from_secs(3))?;
        let clean = training.clean() && testing.clean();
        let (median, p95, concurrent) = if clean {
            let (m, p) = evolution::benchmark_candidate(&bin, &holdout, 2, reps)?;
            let (bad, _) = evolution::concurrent_process_check(&bin, &holdout, processes)?;
            (Some(m), Some(p), Some(bad))
        } else {
            (None, None, None)
        };
        reports.push(RuleResult {
            rule,
            train: training,
            holdout: testing,
            median_us: median,
            p95_us: p95,
            concurrency_failed: concurrent,
            processes,
            eligible_for_review: clean && concurrent == Some(0),
        });
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn twelve_rules_with_distinct_names() {
        let rules = propose_rules();
        assert_eq!(rules.len(), 12);
        assert_eq!(
            rules
                .iter()
                .map(|r| r.label())
                .collect::<BTreeSet<_>>()
                .len(),
            12
        );
    }
    #[test]
    fn no_placeholder_or_known_oracle_in_code() {
        for rule in propose_rules() {
            let src = rule.source();
            assert!(!src.contains("__DEPTH__"));
            assert!(!src.contains("__MISSING__"));
            assert!(!src.contains("__REVOKED__"));
            assert!(!src.contains("COUNT:2\n"));
            assert!(!src.contains("revoked_second_branch"));
        }
    }
    #[test]
    fn fixture_sets_are_distinct() {
        let a = evolution::parse_fixtures(TRAIN).unwrap();
        let b = evolution::parse_fixtures(HOLDOUT).unwrap();
        assert_eq!((a.len(), b.len()), (9, 24));
        verify_suite_disjoint(&a, &b).unwrap();
    }
    #[test]
    fn rejects_duplicate_fixture_inputs_even_with_different_ids() {
        let a = evolution::parse_fixtures("a|1:0:A|1|COUNT:1").unwrap();
        let b = evolution::parse_fixtures("b|1:0:A|1|COUNT:1").unwrap();
        assert!(verify_suite_disjoint(&a, &b).is_err());
    }
    #[test]
    fn bad_budget_configuration_is_rejected() {
        assert!(experiment(Path::new("/unused"), 0, 1).is_err());
        assert!(experiment(Path::new("/unused"), 1, 201).is_err());
    }
    #[test]
    fn fully_traced_candidate_passes_real_process_tests() {
        let root = std::env::temp_dir().join(format!("niccolo-dag-test-{}", std::process::id()));
        let rule = RuleSpec {
            traversal: Traversal::Full,
            missing: MissingPolicy::FailClosed,
            revoked: RevocationPolicy::FailClosed,
        };
        let bin = compile(rule, &root).unwrap();
        let train = evolution::parse_fixtures(TRAIN).unwrap();
        let holdout = evolution::parse_fixtures(HOLDOUT).unwrap();
        let tr = evolution::run_candidate(&bin, &train, Duration::from_secs(3)).unwrap();
        let ho = evolution::run_candidate(&bin, &holdout, Duration::from_secs(3)).unwrap();
        assert!(tr.clean(), "{:?}", tr.failures);
        assert!(ho.clean(), "{:?}", ho.failures);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn seeded_dag_regression_crosscheck_256_cases() {
        use crate::EvidenceId;
        use crate::provenance_dag::{SourceGraph, SourceNode};
        let dir = std::env::temp_dir().join(format!("niccolo-seeded-dag-{}", std::process::id()));
        let rule = RuleSpec {
            traversal: Traversal::Full,
            missing: MissingPolicy::FailClosed,
            revoked: RevocationPolicy::FailClosed,
        };
        let bin = compile(rule, &dir).unwrap();
        let mut seed: u64 = 0xD00D_F00D_9876_1234;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let mut cases = Vec::new();
        for case_id in 0..256 {
            let count = 4 + (rnd() % 12) as u32;
            let mut graph = SourceGraph::default();
            let mut encoded = Vec::new();
            for id in 1..=count {
                let mut parents = BTreeSet::new();
                if id > 1 {
                    let wanted = (rnd() % 4) as usize;
                    for _ in 0..wanted {
                        let target = if rnd() % 13 == 0 {
                            count + 20
                        } else {
                            1 + (rnd() % (id as u64 - 1)) as u32
                        };
                        parents.insert(target);
                    }
                }
                let is_valid = rnd() % 11 != 0;
                let mut node =
                    SourceNode::derived(id as u64, parents.iter().map(|n| EvidenceId(*n as u64)));
                node.admissible = is_valid;
                graph.insert(node).unwrap();
                let text = if parents.is_empty() {
                    "0".to_string()
                } else {
                    parents
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join("+")
                };
                encoded.push(format!("{id}:{text}:{}", if is_valid { "A" } else { "R" }));
            }
            let reports = [
                1 + (rnd() % count as u64) as u32,
                1 + (rnd() % count as u64) as u32,
            ];
            let expected = match graph.assess(
                &reports
                    .iter()
                    .map(|n| EvidenceId(*n as u64))
                    .collect::<Vec<_>>(),
            ) {
                Ok(r) => format!("COUNT:{}", r.traceable_roots.len()),
                Err(_) => "UNKNOWN".to_string(),
            };
            cases.push(Case {
                id: format!("seeded_{case_id}"),
                edges: encoded.join(","),
                observations: reports
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                expected,
            });
        }
        let outcome = evolution::run_candidate(&bin, &cases, Duration::from_secs(5)).unwrap();
        assert!(outcome.clean(), "{:?}", outcome.failures);
        assert_eq!(outcome.total, 256);
        let _ = fs::remove_dir_all(dir);
    }
    #[test]
    fn cheap_candidate_is_refuted() {
        let root = std::env::temp_dir().join(format!("niccolo-dag-naive-{}", std::process::id()));
        let rule = RuleSpec {
            traversal: Traversal::None,
            missing: MissingPolicy::AssumeRoot,
            revoked: RevocationPolicy::Ignore,
        };
        let bin = compile(rule, &root).unwrap();
        let holdout = evolution::parse_fixtures(HOLDOUT).unwrap();
        assert!(
            !evolution::run_candidate(&bin, &holdout, Duration::from_secs(3))
                .unwrap()
                .clean()
        );
        let _ = fs::remove_dir_all(root);
    }
}
