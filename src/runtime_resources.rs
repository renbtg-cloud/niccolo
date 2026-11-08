//! Native Windows/Linux runtime resource observation.
//!
//! This module observes process and system pressure, including cgroup-v2 limits when available.
//! It deliberately does not treat host totals as process entitlements. The
//! snapshot is telemetry only: it does not infer epistemic quality, mutate a
//! cgroup, choose a model, change concurrency or expose infrastructure over HTTP.
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("runtime resource observation is unsupported on this platform")]
    Unsupported,
    #[error("invalid kernel resource data")]
    Invalid,
    #[error("resource observation failed: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceSnapshot {
    pub platform: String,
    pub logical_parallelism: usize,
    pub process_rss_bytes: Option<u64>,
    pub process_peak_rss_bytes: Option<u64>,
    pub cgroup_v2: bool,
    pub cgroup_path: Option<String>,
    pub memory_current_bytes: Option<u64>,
    /// None can mean either an unlimited cgroup or unavailable information;
    /// the observed/unlimited flags below disambiguate it.
    pub memory_limit_bytes: Option<u64>,
    pub memory_limit_observed: bool,
    pub memory_limit_unlimited: bool,
    pub memory_headroom_bytes: Option<u64>,
    pub memory_pressure_basis_points: Option<u16>,
    /// CPU quota normalized to 1/1000 of one CPU. Example: 2500 = 2.5 CPUs.
    pub cpu_quota_millicores: Option<u64>,
    pub cpu_quota_observed: bool,
    pub cpu_quota_unlimited: bool,
    pub cpuset_cpu_count: Option<u32>,
    /// Conservative minimum of observed CPU quota, effective cpuset and
    /// available_parallelism(). This is capacity, not current CPU load.
    pub effective_cpu_capacity_millicores: u64,
    pub cpu_throttled_events: Option<u64>,
    pub cpu_throttled_usec: Option<u64>,
    pub pids_current: Option<u64>,
    pub pids_limit: Option<u64>,
    pub pids_limit_observed: bool,
    pub pids_limit_unlimited: bool,
    pub gpu_observation_implemented: bool,
    pub mutates_resource_limits: bool,
    pub establishes_epistemic_quality: bool,
}

#[derive(Clone, Debug)]
struct LinuxPaths {
    proc_status: PathBuf,
    proc_cgroup: PathBuf,
    cgroup_root: PathBuf,
}

impl Default for LinuxPaths {
    fn default() -> Self {
        Self {
            proc_status: "/proc/self/status".into(),
            proc_cgroup: "/proc/self/cgroup".into(),
            cgroup_root: "/sys/fs/cgroup".into(),
        }
    }
}

fn read_optional(path: &Path) -> Result<Option<String>, ResourceError> {
    match std::fs::read_to_string(path) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn parse_status_kib(status: &str, key: &str) -> Result<Option<u64>, ResourceError> {
    let Some(line) = status.lines().find(|line| line.starts_with(key)) else {
        return Ok(None);
    };
    let mut parts = line[key.len()..].split_whitespace();
    let value = parts
        .next()
        .ok_or(ResourceError::Invalid)?
        .parse::<u64>()
        .map_err(|_| ResourceError::Invalid)?;
    if parts.next() != Some("kB") || parts.next().is_some() {
        return Err(ResourceError::Invalid);
    }
    value
        .checked_mul(1024)
        .map(Some)
        .ok_or(ResourceError::Invalid)
}

fn cgroup_v2_relative_path(text: &str) -> Result<Option<String>, ResourceError> {
    for line in text.lines() {
        let mut parts = line.splitn(3, ':');
        let hierarchy = parts.next().ok_or(ResourceError::Invalid)?;
        let controllers = parts.next().ok_or(ResourceError::Invalid)?;
        let path = parts.next().ok_or(ResourceError::Invalid)?;
        if hierarchy == "0" && controllers.is_empty() {
            if !path.starts_with('/') || path.contains('\0') {
                return Err(ResourceError::Invalid);
            }
            return Ok(Some(path.to_owned()));
        }
    }
    Ok(None)
}

fn safe_cgroup_dir(root: &Path, relative: &str) -> Result<PathBuf, ResourceError> {
    let trimmed = relative.trim_start_matches('/');
    let path = Path::new(trimmed);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
        && !trimmed.is_empty()
    {
        return Err(ResourceError::Invalid);
    }
    Ok(root.join(path))
}

fn parse_scalar(text: Option<&str>) -> Result<Option<u64>, ResourceError> {
    match text {
        Some(value) => value
            .trim()
            .parse::<u64>()
            .map(Some)
            .map_err(|_| ResourceError::Invalid),
        None => Ok(None),
    }
}

fn parse_limit(text: Option<&str>) -> Result<(Option<u64>, bool, bool), ResourceError> {
    let Some(text) = text else {
        return Ok((None, false, false));
    };
    let value = text.trim();
    if value == "max" {
        return Ok((None, true, true));
    }
    let parsed = value.parse::<u64>().map_err(|_| ResourceError::Invalid)?;
    Ok((Some(parsed), true, false))
}

fn parse_cpu_max(text: Option<&str>) -> Result<(Option<u64>, bool, bool), ResourceError> {
    let Some(text) = text else {
        return Ok((None, false, false));
    };
    let mut fields = text.split_whitespace();
    let quota = fields.next().ok_or(ResourceError::Invalid)?;
    let period = fields
        .next()
        .ok_or(ResourceError::Invalid)?
        .parse::<u64>()
        .map_err(|_| ResourceError::Invalid)?;
    if period == 0 || fields.next().is_some() {
        return Err(ResourceError::Invalid);
    }
    if quota == "max" {
        return Ok((None, true, true));
    }
    let quota = quota.parse::<u64>().map_err(|_| ResourceError::Invalid)?;
    let millicores = quota.checked_mul(1000).ok_or(ResourceError::Invalid)? / period;
    Ok((Some(millicores), true, false))
}

fn parse_cpuset_count(text: Option<&str>) -> Result<Option<u32>, ResourceError> {
    let Some(text) = text else {
        return Ok(None);
    };
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let mut total = 0u32;
    for segment in text.split(',') {
        let segment = segment.trim();
        if segment.is_empty() {
            return Err(ResourceError::Invalid);
        }
        if let Some((start, end)) = segment.split_once('-') {
            let start = start.parse::<u32>().map_err(|_| ResourceError::Invalid)?;
            let end = end.parse::<u32>().map_err(|_| ResourceError::Invalid)?;
            if end < start {
                return Err(ResourceError::Invalid);
            }
            total = total
                .checked_add(end - start + 1)
                .ok_or(ResourceError::Invalid)?;
        } else {
            segment.parse::<u32>().map_err(|_| ResourceError::Invalid)?;
            total = total.checked_add(1).ok_or(ResourceError::Invalid)?;
        }
    }
    Ok(Some(total))
}

fn parse_keyed_u64(text: Option<&str>, key: &str) -> Result<Option<u64>, ResourceError> {
    let Some(text) = text else {
        return Ok(None);
    };
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        if fields.next() == Some(key) {
            let value = fields
                .next()
                .ok_or(ResourceError::Invalid)?
                .parse::<u64>()
                .map_err(|_| ResourceError::Invalid)?;
            if fields.next().is_some() {
                return Err(ResourceError::Invalid);
            }
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn observed_capacity(
    logical_parallelism: usize,
    cpu_quota_millicores: Option<u64>,
    cpuset_cpu_count: Option<u32>,
) -> Result<u64, ResourceError> {
    let logical = u64::try_from(logical_parallelism)
        .map_err(|_| ResourceError::Invalid)?
        .checked_mul(1000)
        .ok_or(ResourceError::Invalid)?;
    let cpuset = match cpuset_cpu_count {
        Some(count) => Some(
            u64::from(count)
                .checked_mul(1000)
                .ok_or(ResourceError::Invalid)?,
        ),
        None => None,
    };
    Ok([Some(logical), cpu_quota_millicores, cpuset]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(1000)
        .max(1))
}

struct RawResourceObservation<'a> {
    logical_parallelism: usize,
    proc_status: &'a str,
    cgroup_relative: Option<String>,
    memory_current: Option<&'a str>,
    memory_max: Option<&'a str>,
    cpu_max: Option<&'a str>,
    cpuset: Option<&'a str>,
    cpu_stat: Option<&'a str>,
    pids_current: Option<&'a str>,
    pids_max: Option<&'a str>,
}

fn snapshot_from(raw: RawResourceObservation<'_>) -> Result<ResourceSnapshot, ResourceError> {
    let RawResourceObservation {
        logical_parallelism,
        proc_status,
        cgroup_relative,
        memory_current,
        memory_max,
        cpu_max,
        cpuset,
        cpu_stat,
        pids_current,
        pids_max,
    } = raw;
    if logical_parallelism == 0 {
        return Err(ResourceError::Invalid);
    }
    let process_rss_bytes = parse_status_kib(proc_status, "VmRSS:")?;
    let process_peak_rss_bytes = parse_status_kib(proc_status, "VmHWM:")?;
    let memory_current_bytes = parse_scalar(memory_current)?;
    let (memory_limit_bytes, memory_limit_observed, memory_limit_unlimited) =
        parse_limit(memory_max)?;
    let memory_headroom_bytes = match (memory_current_bytes, memory_limit_bytes) {
        (Some(current), Some(limit)) => Some(limit.saturating_sub(current)),
        _ => None,
    };
    let memory_pressure_basis_points = match (memory_current_bytes, memory_limit_bytes) {
        (Some(current), Some(limit)) if limit > 0 => {
            let points = (u128::from(current) * 10_000)
                .checked_div(u128::from(limit))
                .unwrap_or(10_000)
                .min(10_000);
            Some(u16::try_from(points).map_err(|_| ResourceError::Invalid)?)
        }
        _ => None,
    };
    let (cpu_quota_millicores, cpu_quota_observed, cpu_quota_unlimited) = parse_cpu_max(cpu_max)?;
    let cpuset_cpu_count = parse_cpuset_count(cpuset)?;
    let effective_cpu_capacity_millicores =
        observed_capacity(logical_parallelism, cpu_quota_millicores, cpuset_cpu_count)?;
    let cpu_throttled_events = parse_keyed_u64(cpu_stat, "nr_throttled")?;
    let cpu_throttled_usec = parse_keyed_u64(cpu_stat, "throttled_usec")?;
    let pids_current = parse_scalar(pids_current)?;
    let (pids_limit, pids_limit_observed, pids_limit_unlimited) = parse_limit(pids_max)?;
    Ok(ResourceSnapshot {
        platform: "linux".into(),
        logical_parallelism,
        process_rss_bytes,
        process_peak_rss_bytes,
        cgroup_v2: cgroup_relative.is_some(),
        cgroup_path: cgroup_relative,
        memory_current_bytes,
        memory_limit_bytes,
        memory_limit_observed,
        memory_limit_unlimited,
        memory_headroom_bytes,
        memory_pressure_basis_points,
        cpu_quota_millicores,
        cpu_quota_observed,
        cpu_quota_unlimited,
        cpuset_cpu_count,
        effective_cpu_capacity_millicores,
        cpu_throttled_events,
        cpu_throttled_usec,
        pids_current,
        pids_limit,
        pids_limit_observed,
        pids_limit_unlimited,
        gpu_observation_implemented: false,
        mutates_resource_limits: false,
        establishes_epistemic_quality: false,
    })
}

#[cfg(target_os = "linux")]
fn observe_linux_with(paths: &LinuxPaths) -> Result<ResourceSnapshot, ResourceError> {
    let proc_status = std::fs::read_to_string(&paths.proc_status)?;
    let cgroup_text = read_optional(&paths.proc_cgroup)?;
    let cgroup_relative = match cgroup_text {
        Some(text) => cgroup_v2_relative_path(&text)?,
        None => None,
    };
    let cgroup_dir = cgroup_relative
        .as_deref()
        .map(|path| safe_cgroup_dir(&paths.cgroup_root, path))
        .transpose()?;
    let read = |name: &str| -> Result<Option<String>, ResourceError> {
        match &cgroup_dir {
            Some(dir) => read_optional(&dir.join(name)),
            None => Ok(None),
        }
    };
    let memory_current = read("memory.current")?;
    let memory_max = read("memory.max")?;
    let cpu_max = read("cpu.max")?;
    let cpuset = read("cpuset.cpus.effective")?;
    let cpu_stat = read("cpu.stat")?;
    let pids_current = read("pids.current")?;
    let pids_max = read("pids.max")?;
    let logical_parallelism = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1);
    snapshot_from(RawResourceObservation {
        logical_parallelism,
        proc_status: &proc_status,
        cgroup_relative,
        memory_current: memory_current.as_deref(),
        memory_max: memory_max.as_deref(),
        cpu_max: cpu_max.as_deref(),
        cpuset: cpuset.as_deref(),
        cpu_stat: cpu_stat.as_deref(),
        pids_current: pids_current.as_deref(),
        pids_max: pids_max.as_deref(),
    })
}

/// Windows host telemetry. Host memory pressure informs local work admission,
/// but it must never be presented as the process's Job Object memory quota.
#[cfg(windows)]
fn observe_windows() -> Result<ResourceSnapshot, ResourceError> {
    use std::{ffi::c_void, mem::size_of};
    type Handle = *mut c_void;

    #[repr(C)]
    struct MemoryStatus {
        length: u32,
        load_percent: u32,
        total_physical: u64,
        available_physical: u64,
        total_pagefile: u64,
        available_pagefile: u64,
        total_virtual: u64,
        available_virtual: u64,
        available_extended_virtual: u64,
    }

    #[repr(C)]
    struct ProcessMemory {
        size: u32,
        page_fault_count: u32,
        peak_working_set: usize,
        working_set: usize,
        quota_peak_paged_pool: usize,
        quota_paged_pool: usize,
        quota_peak_nonpaged_pool: usize,
        quota_nonpaged_pool: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(status: *mut MemoryStatus) -> i32;
        fn GetCurrentProcess() -> Handle;
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(process: Handle, counters: *mut ProcessMemory, size: u32) -> i32;
    }

    let mut mem = MemoryStatus {
        length: size_of::<MemoryStatus>() as u32,
        load_percent: 0,
        total_physical: 0,
        available_physical: 0,
        total_pagefile: 0,
        available_pagefile: 0,
        total_virtual: 0,
        available_virtual: 0,
        available_extended_virtual: 0,
    };
    // SAFETY: initialized writable MEMORYSTATUSEX, including required byte size.
    if unsafe { GlobalMemoryStatusEx(&mut mem) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut process_mem = ProcessMemory {
        size: size_of::<ProcessMemory>() as u32,
        page_fault_count: 0,
        peak_working_set: 0,
        working_set: 0,
        quota_peak_paged_pool: 0,
        quota_paged_pool: 0,
        quota_peak_nonpaged_pool: 0,
        quota_nonpaged_pool: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };
    // SAFETY: GetCurrentProcess is a valid pseudo-handle; the destination is
    // correctly laid out and its size matches the declared Win32 structure.
    let process_mem_size = process_mem.size;
    if unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut process_mem, process_mem_size) } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }

    let logical_parallelism = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    let capacity = u64::try_from(logical_parallelism)
        .map_err(|_| ResourceError::Invalid)?
        .checked_mul(1000)
        .ok_or(ResourceError::Invalid)?;
    Ok(ResourceSnapshot {
        platform: "windows".into(),
        logical_parallelism,
        process_rss_bytes: Some(process_mem.working_set as u64),
        process_peak_rss_bytes: Some(process_mem.peak_working_set as u64),
        cgroup_v2: false,
        cgroup_path: None,
        memory_current_bytes: None,
        memory_limit_bytes: None,
        memory_limit_observed: false,
        memory_limit_unlimited: false,
        // This is free physical memory, NOT remaining Job Object memory quota.
        memory_headroom_bytes: Some(mem.available_physical),
        // System-wide pressure, not a claim about the current process quota.
        memory_pressure_basis_points: Some(
            u16::try_from(mem.load_percent.min(100) * 100).map_err(|_| ResourceError::Invalid)?,
        ),
        cpu_quota_millicores: None,
        cpu_quota_observed: false,
        cpu_quota_unlimited: false,
        cpuset_cpu_count: None,
        effective_cpu_capacity_millicores: capacity,
        cpu_throttled_events: None,
        cpu_throttled_usec: None,
        pids_current: None,
        pids_limit: None,
        pids_limit_observed: false,
        pids_limit_unlimited: false,
        gpu_observation_implemented: false,
        mutates_resource_limits: false,
        establishes_epistemic_quality: false,
    })
}

/// Observe current process/cgroup limits without modifying the runtime.
///
/// Linux observes effective cgroup limits; Windows observes system memory
/// pressure and the current process working set, without claiming to know
/// unqueried Job Object quotas. Missing information remains explicit.
pub fn observe() -> Result<ResourceSnapshot, ResourceError> {
    #[cfg(target_os = "linux")]
    {
        observe_linux_with(&LinuxPaths::default())
    }
    #[cfg(windows)]
    {
        observe_windows()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        Err(ResourceError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = "Name:\tniccolo\nVmHWM:\t2048 kB\nVmRSS:\t1536 kB\nThreads:\t8\n";

    #[test]
    fn parses_realistic_cgroup_v2_envelope_without_using_host_totals() {
        let snapshot = snapshot_from(RawResourceObservation {
            logical_parallelism: 8,
            proc_status: STATUS,
            cgroup_relative: Some("/tenant/niccolo".into()),
            memory_current: Some("3221225472\n"),
            memory_max: Some("4294967296\n"),
            cpu_max: Some("250000 100000\n"),
            cpuset: Some("0-3,6\n"),
            cpu_stat: Some("usage_usec 100\nnr_throttled 7\nthrottled_usec 12345\n"),
            pids_current: Some("18\n"),
            pids_max: Some("256\n"),
        })
        .unwrap();
        assert_eq!(snapshot.process_rss_bytes, Some(1536 * 1024));
        assert_eq!(snapshot.process_peak_rss_bytes, Some(2048 * 1024));
        assert_eq!(snapshot.memory_headroom_bytes, Some(1_073_741_824));
        assert_eq!(snapshot.memory_pressure_basis_points, Some(7500));
        assert_eq!(snapshot.cpu_quota_millicores, Some(2500));
        assert_eq!(snapshot.cpuset_cpu_count, Some(5));
        assert_eq!(snapshot.effective_cpu_capacity_millicores, 2500);
        assert_eq!(snapshot.cpu_throttled_events, Some(7));
        assert_eq!(snapshot.cpu_throttled_usec, Some(12345));
        assert_eq!(snapshot.pids_limit, Some(256));
        assert!(!snapshot.gpu_observation_implemented);
        assert!(!snapshot.mutates_resource_limits);
        assert!(!snapshot.establishes_epistemic_quality);
    }

    #[test]
    fn unlimited_limits_are_distinct_from_unobserved_limits() {
        let unlimited = snapshot_from(RawResourceObservation {
            logical_parallelism: 4,
            proc_status: STATUS,
            cgroup_relative: Some("/".into()),
            memory_current: Some("1000"),
            memory_max: Some("max"),
            cpu_max: Some("max 100000"),
            cpuset: Some("0-1"),
            cpu_stat: None,
            pids_current: Some("3"),
            pids_max: Some("max"),
        })
        .unwrap();
        assert_eq!(unlimited.memory_limit_bytes, None);
        assert!(unlimited.memory_limit_observed);
        assert!(unlimited.memory_limit_unlimited);
        assert_eq!(unlimited.cpu_quota_millicores, None);
        assert!(unlimited.cpu_quota_observed);
        assert!(unlimited.cpu_quota_unlimited);
        assert_eq!(unlimited.pids_limit, None);
        assert!(unlimited.pids_limit_observed);
        assert!(unlimited.pids_limit_unlimited);
        assert_eq!(unlimited.effective_cpu_capacity_millicores, 2000);

        let unknown = snapshot_from(RawResourceObservation {
            logical_parallelism: 4,
            proc_status: STATUS,
            cgroup_relative: None,
            memory_current: None,
            memory_max: None,
            cpu_max: None,
            cpuset: None,
            cpu_stat: None,
            pids_current: None,
            pids_max: None,
        })
        .unwrap();
        assert!(!unknown.memory_limit_observed);
        assert!(!unknown.cpu_quota_observed);
        assert!(!unknown.pids_limit_observed);
        assert_eq!(unknown.effective_cpu_capacity_millicores, 4000);
    }

    #[test]
    fn cgroup_path_cannot_escape_observation_root() {
        assert!(safe_cgroup_dir(Path::new("/safe"), "/a/b").is_ok());
        assert!(safe_cgroup_dir(Path::new("/safe"), "/../escape").is_err());
        assert!(safe_cgroup_dir(Path::new("/safe"), "/a/../../escape").is_err());
    }

    #[test]
    fn cpuset_parser_handles_ranges_and_rejects_malformed_data() {
        assert_eq!(parse_cpuset_count(Some("0-3,6,8-9")).unwrap(), Some(7));
        assert_eq!(parse_cpuset_count(Some("")).unwrap(), None);
        assert!(parse_cpuset_count(Some("4-2")).is_err());
        assert!(parse_cpuset_count(Some("0,,2")).is_err());
    }

    #[test]
    fn malformed_kernel_units_and_zero_period_fail_closed() {
        assert!(parse_status_kib("VmRSS: 3 MB", "VmRSS:").is_err());
        assert!(parse_cpu_max(Some("1000 0")).is_err());
        assert!(parse_cpu_max(Some("1000")).is_err());
        assert!(parse_limit(Some("-1")).is_err());
    }

    #[test]
    fn memory_usage_above_limit_clamps_headroom_and_pressure() {
        let snapshot = snapshot_from(RawResourceObservation {
            logical_parallelism: 1,
            proc_status: STATUS,
            cgroup_relative: Some("/".into()),
            memory_current: Some("110"),
            memory_max: Some("100"),
            cpu_max: Some("100000 100000"),
            cpuset: Some("0"),
            cpu_stat: None,
            pids_current: None,
            pids_max: None,
        })
        .unwrap();
        assert_eq!(snapshot.memory_headroom_bytes, Some(0));
        assert_eq!(snapshot.memory_pressure_basis_points, Some(10_000));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn live_linux_observation_is_read_only_and_reports_process_capacity() {
        let snapshot = observe().unwrap();
        assert_eq!(snapshot.platform, "linux");
        assert!(snapshot.logical_parallelism >= 1);
        assert!(snapshot.effective_cpu_capacity_millicores >= 1);
        assert!(!snapshot.mutates_resource_limits);
        assert!(!snapshot.establishes_epistemic_quality);
        assert!(!snapshot.gpu_observation_implemented);
    }

    #[cfg(windows)]
    #[test]
    fn live_windows_telemetry_is_read_only_and_distinguishes_host_from_job_limits() {
        let snapshot = observe().unwrap();
        assert_eq!(snapshot.platform, "windows");
        assert!(snapshot.logical_parallelism >= 1);
        assert!(snapshot.process_rss_bytes.is_some_and(|bytes| bytes > 0));
        assert!(snapshot.memory_headroom_bytes.is_some());
        assert!(
            snapshot
                .memory_pressure_basis_points
                .is_some_and(|points| points <= 10_000)
        );
        assert!(!snapshot.memory_limit_observed);
        assert!(!snapshot.cpu_quota_observed);
        assert!(!snapshot.mutates_resource_limits);
        assert!(!snapshot.establishes_epistemic_quality);
    }

    #[test]
    fn cgroup_v2_parser_ignores_legacy_lines_and_requires_absolute_path() {
        assert_eq!(
            cgroup_v2_relative_path("9:cpu:/legacy\n0::/service.slice/x\n").unwrap(),
            Some("/service.slice/x".into())
        );
        assert!(cgroup_v2_relative_path("0::relative").is_err());
    }
}
