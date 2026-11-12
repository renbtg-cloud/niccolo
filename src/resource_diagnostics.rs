//! Sanitized runtime resource diagnostic suitable for an authenticated operator surface.
//!
//! Raw cgroup paths and other infrastructure identifiers are deliberately omitted.
//! The diagnostic describes operational capacity and policy advice only; it does
//! not establish reasoning quality, confidence, completeness, or truth.
use crate::{
    resource_policy::{self, ResourcePolicy, ResourcePolicyError},
    runtime_resources::{self, ResourceError, ResourceSnapshot},
};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ResourceDiagnosticError {
    #[error("resource observation unavailable: {0}")]
    Observation(#[from] ResourceError),
    #[error("resource policy unavailable: {0}")]
    Policy(#[from] ResourcePolicyError),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ResourceDiagnostic {
    pub platform: String,
    pub logical_parallelism: usize,
    pub process_rss_bytes: Option<u64>,
    pub process_peak_rss_bytes: Option<u64>,
    pub memory_current_bytes: Option<u64>,
    pub memory_limit_bytes: Option<u64>,
    pub memory_limit_observed: bool,
    pub memory_limit_unlimited: bool,
    pub memory_headroom_bytes: Option<u64>,
    pub memory_pressure_basis_points: Option<u16>,
    pub effective_cpu_capacity_millicores: u64,
    pub cpu_quota_observed: bool,
    pub cpu_quota_unlimited: bool,
    pub cpu_throttled_events: Option<u64>,
    pub cpu_throttled_usec: Option<u64>,
    pub pids_current: Option<u64>,
    pub pids_limit: Option<u64>,
    pub pids_limit_observed: bool,
    pub pids_limit_unlimited: bool,
    pub pressure: resource_policy::ResourcePressure,
    pub observed_parallel_ceiling: u16,
    pub recommended_parallel_operations: u16,
    pub apply_concurrency_automatically: bool,
    pub defer_nonessential_new_work: bool,
    pub advice_reasons: Vec<String>,
    pub mutates_resource_limits: bool,
    pub establishes_epistemic_quality: bool,
    pub advice_is_operational_only: bool,
}

pub fn from_snapshot(
    snapshot: &ResourceSnapshot,
    policy: &ResourcePolicy,
) -> Result<ResourceDiagnostic, ResourceDiagnosticError> {
    let advice = resource_policy::advise(snapshot, policy)?;
    Ok(ResourceDiagnostic {
        platform: snapshot.platform.clone(),
        logical_parallelism: snapshot.logical_parallelism,
        process_rss_bytes: snapshot.process_rss_bytes,
        process_peak_rss_bytes: snapshot.process_peak_rss_bytes,
        memory_current_bytes: snapshot.memory_current_bytes,
        memory_limit_bytes: snapshot.memory_limit_bytes,
        memory_limit_observed: snapshot.memory_limit_observed,
        memory_limit_unlimited: snapshot.memory_limit_unlimited,
        memory_headroom_bytes: snapshot.memory_headroom_bytes,
        memory_pressure_basis_points: snapshot.memory_pressure_basis_points,
        effective_cpu_capacity_millicores: snapshot.effective_cpu_capacity_millicores,
        cpu_quota_observed: snapshot.cpu_quota_observed,
        cpu_quota_unlimited: snapshot.cpu_quota_unlimited,
        cpu_throttled_events: snapshot.cpu_throttled_events,
        cpu_throttled_usec: snapshot.cpu_throttled_usec,
        pids_current: snapshot.pids_current,
        pids_limit: snapshot.pids_limit,
        pids_limit_observed: snapshot.pids_limit_observed,
        pids_limit_unlimited: snapshot.pids_limit_unlimited,
        pressure: advice.pressure,
        observed_parallel_ceiling: advice.observed_parallel_ceiling,
        recommended_parallel_operations: advice.recommended_parallel_operations,
        apply_concurrency_automatically: advice.apply_concurrency_automatically,
        defer_nonessential_new_work: advice.defer_nonessential_new_work,
        advice_reasons: advice.reasons,
        mutates_resource_limits: false,
        establishes_epistemic_quality: false,
        advice_is_operational_only: true,
    })
}

pub fn current() -> Result<ResourceDiagnostic, ResourceDiagnosticError> {
    let snapshot = runtime_resources::observe()?;
    from_snapshot(&snapshot, &ResourcePolicy::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource_policy::{ResourceManagementMode, ResourcePressure};

    fn snapshot() -> ResourceSnapshot {
        ResourceSnapshot {
            platform: "linux".into(),
            logical_parallelism: 8,
            process_rss_bytes: Some(512),
            process_peak_rss_bytes: Some(1024),
            cgroup_v2: true,
            cgroup_path: Some("/tenant-sensitive-path".into()),
            memory_current_bytes: Some(900),
            memory_limit_bytes: Some(1000),
            memory_limit_observed: true,
            memory_limit_unlimited: false,
            memory_headroom_bytes: Some(100),
            memory_pressure_basis_points: Some(9000),
            cpu_quota_millicores: Some(2000),
            cpu_quota_observed: true,
            cpu_quota_unlimited: false,
            cpuset_cpu_count: Some(4),
            effective_cpu_capacity_millicores: 2000,
            cpu_throttled_events: Some(9),
            cpu_throttled_usec: Some(1200),
            pids_current: Some(4),
            pids_limit: Some(32),
            pids_limit_observed: true,
            pids_limit_unlimited: false,
            gpu_observation_implemented: false,
            mutates_resource_limits: false,
            establishes_epistemic_quality: false,
        }
    }

    #[test]
    fn diagnostic_is_operational_and_omits_raw_infrastructure_identity() {
        let policy = ResourcePolicy {
            mode: ResourceManagementMode::ExternallyManaged,
            reserved_memory_bytes: 0,
            ..ResourcePolicy::default()
        };
        let diagnostic = from_snapshot(&snapshot(), &policy).unwrap();
        assert_eq!(diagnostic.pressure, ResourcePressure::Constrained);
        assert_eq!(diagnostic.observed_parallel_ceiling, 4);
        assert_eq!(diagnostic.recommended_parallel_operations, 2);
        assert!(!diagnostic.apply_concurrency_automatically);
        assert!(!diagnostic.mutates_resource_limits);
        assert!(!diagnostic.establishes_epistemic_quality);
        assert!(diagnostic.advice_is_operational_only);
        let json = serde_json::to_value(&diagnostic).unwrap();
        assert!(json.get("cgroup_path").is_none());
        assert!(!json.to_string().contains("tenant-sensitive-path"));
    }
}
