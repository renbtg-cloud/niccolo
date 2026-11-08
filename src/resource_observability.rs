//! Runtime resource observation for adaptive execution.
//!
//! ARM-001 observes effective process/container limits without pretending that
//! machine capacity is epistemic quality. Linux cgroup v2 is supported first;
//! non-Linux builds still compile and expose the portable boundary.
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSnapshot {
    pub platform: String,
    /// Runtime scheduling hint, generally affinity-aware but not a promise of
    /// sustained CPU entitlement.
    pub runtime_parallelism_hint: usize,
    /// Host-visible memory total when available.
    pub host_memory_total_bytes: Option<u64>,
    /// Current process resident set size when available.
    pub process_rss_bytes: Option<u64>,
    /// cgroup v2 hard memory ceiling; None means absent, unreadable, or max.
    pub cgroup_memory_limit_bytes: Option<u64>,
    /// cgroup v2 current memory usage when available.
    pub cgroup_memory_current_bytes: Option<u64>,
    /// Minimum of known host/cgroup ceilings, if any.
    pub effective_memory_limit_bytes: Option<u64>,
    /// CPU quota expressed in thousandths of one core. 1500 = 1.5 cores.
    pub cgroup_cpu_quota_millis: Option<u64>,
    /// Minimum of runtime parallelism hint and cgroup quota, in thousandths of a core.
    pub effective_cpu_limit_millis: Option<u64>,
    pub cgroup_v2_detected: bool,
    pub capacity_is_epistemic_quality_score: bool,
    pub controls_resources: bool,
}

fn parse_kib_line(contents: &str, key: &str) -> Option<u64> {
    let line = contents.lines().find(|line| line.starts_with(key))?;
    let mut parts = line.split_whitespace();
    let _ = parts.next()?;
    let kib = parts.next()?.parse::<u64>().ok()?;
    kib.checked_mul(1024)
}

fn parse_memory_limit(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if trimmed == "max" {
        None
    } else {
        trimmed.parse::<u64>().ok()
    }
}

fn parse_cpu_max(value: &str) -> Option<u64> {
    let mut fields = value.split_whitespace();
    let quota = fields.next()?;
    let period = fields.next()?.parse::<u64>().ok()?;
    if quota == "max" || period == 0 {
        return None;
    }
    let quota = quota.parse::<u64>().ok()?;
    quota.checked_mul(1000)?.checked_div(period)
}

fn min_known(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

#[cfg(target_os = "linux")]
fn cgroup_v2_dir() -> Option<PathBuf> {
    let membership = fs::read_to_string("/proc/self/cgroup").ok()?;
    let relative = membership.lines().find_map(|line| {
        let mut parts = line.splitn(3, ':');
        let hierarchy = parts.next()?;
        let controllers = parts.next()?;
        let path = parts.next()?;
        if hierarchy == "0" && controllers.is_empty() {
            Some(path.trim_start_matches('/'))
        } else {
            None
        }
    })?;
    let mut base = PathBuf::from("/sys/fs/cgroup");
    if !relative.is_empty() {
        base.push(relative);
    }
    base.join("cgroup.controllers").exists().then_some(base)
}

#[cfg(not(target_os = "linux"))]
fn cgroup_v2_dir() -> Option<PathBuf> {
    None
}

pub fn observe() -> ResourceSnapshot {
    let runtime_parallelism_hint = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    #[cfg(target_os = "linux")]
    let host_memory_total_bytes = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| parse_kib_line(&text, "MemTotal:"));
    #[cfg(not(target_os = "linux"))]
    let host_memory_total_bytes = None;

    #[cfg(target_os = "linux")]
    let process_rss_bytes = fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| parse_kib_line(&text, "VmRSS:"));
    #[cfg(not(target_os = "linux"))]
    let process_rss_bytes = None;

    let cgroup = cgroup_v2_dir();
    let cgroup_memory_limit_bytes = cgroup
        .as_ref()
        .and_then(|dir| fs::read_to_string(dir.join("memory.max")).ok())
        .and_then(|value| parse_memory_limit(&value));
    let cgroup_memory_current_bytes = cgroup
        .as_ref()
        .and_then(|dir| fs::read_to_string(dir.join("memory.current")).ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    let cgroup_cpu_quota_millis = cgroup
        .as_ref()
        .and_then(|dir| fs::read_to_string(dir.join("cpu.max")).ok())
        .and_then(|value| parse_cpu_max(&value));
    let affinity_hint_millis = u64::try_from(runtime_parallelism_hint)
        .ok()
        .and_then(|cores| cores.checked_mul(1000));

    ResourceSnapshot {
        platform: std::env::consts::OS.to_owned(),
        runtime_parallelism_hint,
        host_memory_total_bytes,
        process_rss_bytes,
        cgroup_memory_limit_bytes,
        cgroup_memory_current_bytes,
        effective_memory_limit_bytes: min_known(
            host_memory_total_bytes,
            cgroup_memory_limit_bytes,
        ),
        cgroup_cpu_quota_millis,
        effective_cpu_limit_millis: min_known(affinity_hint_millis, cgroup_cpu_quota_millis),
        cgroup_v2_detected: cgroup.is_some(),
        capacity_is_epistemic_quality_score: false,
        controls_resources: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_memory_lines_without_unit_confusion() {
        assert_eq!(
            parse_kib_line("MemTotal:       16384 kB\n", "MemTotal:"),
            Some(16 * 1024 * 1024)
        );
        assert_eq!(parse_kib_line("VmRSS: 42 kB\n", "VmRSS:"), Some(42 * 1024));
    }

    #[test]
    fn parses_cgroup_v2_cpu_quota_as_milli_cores() {
        assert_eq!(parse_cpu_max("200000 100000"), Some(2000));
        assert_eq!(parse_cpu_max("150000 100000"), Some(1500));
        assert_eq!(parse_cpu_max("max 100000"), None);
        assert_eq!(parse_cpu_max("100000 0"), None);
    }

    #[test]
    fn max_memory_is_not_misreported_as_a_numeric_limit() {
        assert_eq!(parse_memory_limit("max\n"), None);
        assert_eq!(parse_memory_limit("1048576\n"), Some(1_048_576));
    }

    #[test]
    fn effective_capacity_uses_the_tightest_known_ceiling() {
        assert_eq!(min_known(Some(32), Some(8)), Some(8));
        assert_eq!(min_known(Some(32), None), Some(32));
        assert_eq!(min_known(None, Some(8)), Some(8));
        assert_eq!(min_known(None, None), None);
    }

    #[test]
    fn runtime_snapshot_never_claims_capacity_is_reasoning_quality() {
        let snapshot = observe();
        assert!(snapshot.runtime_parallelism_hint >= 1);
        assert!(!snapshot.capacity_is_epistemic_quality_score);
        assert!(!snapshot.controls_resources);
        if let (Some(current), Some(limit)) = (
            snapshot.cgroup_memory_current_bytes,
            snapshot.cgroup_memory_limit_bytes,
        ) {
            assert!(current > 0 || limit > 0);
        }
    }
}
