//! ARM-002 resource-management policy.
//!
//! This module converts observed capacity into conservative advice. The worker and
//! HTTP background executors may apply its concurrency recommendation, but this
//! policy layer does not mutate cgroups, select models, or certify epistemic quality.
use crate::runtime_resources::ResourceSnapshot;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourceManagementMode {
    SelfManaged,
    ExternallyManaged,
    Hybrid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourcePolicy {
    pub mode: ResourceManagementMode,
    pub operator_max_parallel_operations: u16,
    pub min_cpu_millicores_per_operation: u64,
    pub reserved_memory_bytes: u64,
    pub high_memory_pressure_basis_points: u16,
    pub critical_memory_pressure_basis_points: u16,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self {
            mode: ResourceManagementMode::SelfManaged,
            operator_max_parallel_operations: 8,
            min_cpu_millicores_per_operation: 500,
            reserved_memory_bytes: 256 * 1024 * 1024,
            high_memory_pressure_basis_points: 8_500,
            critical_memory_pressure_basis_points: 9_500,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourcePressure {
    Unknown,
    Normal,
    Constrained,
    Critical,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAdvice {
    pub mode: ResourceManagementMode,
    pub pressure: ResourcePressure,
    pub observed_parallel_ceiling: u16,
    pub recommended_parallel_operations: u16,
    pub apply_concurrency_automatically: bool,
    pub defer_nonessential_new_work: bool,
    pub reasons: Vec<String>,
    pub mutates_resource_limits: bool,
    pub establishes_epistemic_quality: bool,
    pub allows_disclosed_epistemic_degradation: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResourcePolicyError {
    #[error("invalid resource policy")]
    Invalid,
}

pub fn validate(policy: &ResourcePolicy) -> Result<(), ResourcePolicyError> {
    if policy.operator_max_parallel_operations == 0
        || policy.operator_max_parallel_operations > 512
        || policy.min_cpu_millicores_per_operation == 0
        || policy.high_memory_pressure_basis_points > 10_000
        || policy.critical_memory_pressure_basis_points > 10_000
        || policy.high_memory_pressure_basis_points >= policy.critical_memory_pressure_basis_points
    {
        return Err(ResourcePolicyError::Invalid);
    }
    Ok(())
}

fn cpu_ceiling(snapshot: &ResourceSnapshot, policy: &ResourcePolicy) -> u16 {
    let raw = snapshot
        .effective_cpu_capacity_millicores
        .checked_div(policy.min_cpu_millicores_per_operation)
        .unwrap_or(0)
        .max(1);
    u16::try_from(raw)
        .unwrap_or(u16::MAX)
        .min(policy.operator_max_parallel_operations)
}

fn pid_headroom_ceiling(snapshot: &ResourceSnapshot, policy: &ResourcePolicy) -> Option<u16> {
    if !snapshot.pids_limit_observed || snapshot.pids_limit_unlimited {
        return None;
    }
    let (Some(current), Some(limit)) = (snapshot.pids_current, snapshot.pids_limit) else {
        return None;
    };
    let available = limit.saturating_sub(current).max(1);
    Some(
        u16::try_from(available)
            .unwrap_or(u16::MAX)
            .min(policy.operator_max_parallel_operations),
    )
}

fn pids_exhausted(snapshot: &ResourceSnapshot) -> bool {
    snapshot.pids_limit_observed
        && !snapshot.pids_limit_unlimited
        && snapshot
            .pids_current
            .zip(snapshot.pids_limit)
            .is_some_and(|(current, limit)| current >= limit)
}

fn memory_pressure(snapshot: &ResourceSnapshot, policy: &ResourcePolicy) -> ResourcePressure {
    let Some(points) = snapshot.memory_pressure_basis_points else {
        return ResourcePressure::Unknown;
    };
    if points >= policy.critical_memory_pressure_basis_points
        || snapshot
            .memory_headroom_bytes
            .is_some_and(|bytes| bytes < policy.reserved_memory_bytes)
    {
        ResourcePressure::Critical
    } else if points >= policy.high_memory_pressure_basis_points {
        ResourcePressure::Constrained
    } else {
        ResourcePressure::Normal
    }
}

pub fn advise(
    snapshot: &ResourceSnapshot,
    policy: &ResourcePolicy,
) -> Result<ResourceAdvice, ResourcePolicyError> {
    validate(policy)?;
    let memory_pressure = memory_pressure(snapshot, policy);
    let pid_ceiling = pid_headroom_ceiling(snapshot, policy);
    let cpu_ceiling = cpu_ceiling(snapshot, policy);
    let observed_parallel_ceiling = pid_ceiling.map_or(cpu_ceiling, |pids| cpu_ceiling.min(pids));
    let pid_exhausted = pids_exhausted(snapshot);
    let pressure = if pid_exhausted {
        ResourcePressure::Critical
    } else {
        memory_pressure
    };
    let mut recommended = observed_parallel_ceiling;
    let mut reasons = Vec::new();

    if cpu_ceiling < policy.operator_max_parallel_operations {
        reasons.push("CPU_CAPACITY".into());
    }
    if pid_ceiling.is_some_and(|pids| pids < cpu_ceiling) {
        reasons.push("PID_CAPACITY".into());
    }

    match memory_pressure {
        ResourcePressure::Critical => {
            recommended = 1;
            reasons.push("CRITICAL_MEMORY_PRESSURE".into());
        }
        ResourcePressure::Constrained => {
            recommended = recommended.div_ceil(2).max(1);
            reasons.push("HIGH_MEMORY_PRESSURE".into());
        }
        ResourcePressure::Normal => {}
        ResourcePressure::Unknown => reasons.push("MEMORY_PRESSURE_UNKNOWN".into()),
    }
    if pid_exhausted {
        recommended = 1;
        reasons.push("PID_LIMIT_EXHAUSTED".into());
    }

    let apply_concurrency_automatically =
        !matches!(policy.mode, ResourceManagementMode::ExternallyManaged);
    if matches!(policy.mode, ResourceManagementMode::ExternallyManaged) {
        reasons.push("OPERATOR_DELEGATED_CONTROL".into());
    }

    Ok(ResourceAdvice {
        mode: policy.mode,
        pressure,
        observed_parallel_ceiling,
        recommended_parallel_operations: recommended.max(1),
        apply_concurrency_automatically,
        defer_nonessential_new_work: pressure == ResourcePressure::Critical,
        reasons,
        mutates_resource_limits: false,
        establishes_epistemic_quality: false,
        allows_disclosed_epistemic_degradation: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(cpu: u64, pressure: Option<u16>, headroom: Option<u64>) -> ResourceSnapshot {
        ResourceSnapshot {
            platform: "linux".into(),
            logical_parallelism: 8,
            process_rss_bytes: Some(10),
            process_peak_rss_bytes: Some(20),
            cgroup_v2: true,
            cgroup_path: Some("/niccolo".into()),
            memory_current_bytes: Some(10),
            memory_limit_bytes: Some(100),
            memory_limit_observed: true,
            memory_limit_unlimited: false,
            memory_headroom_bytes: headroom,
            memory_pressure_basis_points: pressure,
            cpu_quota_millicores: Some(cpu),
            cpu_quota_observed: true,
            cpu_quota_unlimited: false,
            cpuset_cpu_count: Some(8),
            effective_cpu_capacity_millicores: cpu,
            cpu_throttled_events: None,
            cpu_throttled_usec: None,
            pids_current: None,
            pids_limit: None,
            pids_limit_observed: false,
            pids_limit_unlimited: false,
            gpu_observation_implemented: false,
            mutates_resource_limits: false,
            establishes_epistemic_quality: false,
        }
    }

    #[test]
    fn default_is_self_managed_but_only_advisory_at_this_layer() {
        let p = ResourcePolicy::default();
        assert_eq!(p.mode, ResourceManagementMode::SelfManaged);
        let a = advise(&snapshot(4_000, Some(2_000), Some(u64::MAX)), &p).unwrap();
        assert!(a.apply_concurrency_automatically);
        assert!(!a.mutates_resource_limits);
        assert!(!a.establishes_epistemic_quality);
        assert!(a.allows_disclosed_epistemic_degradation);
    }

    #[test]
    fn cpu_entitlement_caps_parallel_advice() {
        let p = ResourcePolicy::default();
        let a = advise(&snapshot(1_500, Some(2_000), Some(u64::MAX)), &p).unwrap();
        assert_eq!(a.observed_parallel_ceiling, 3);
        assert_eq!(a.recommended_parallel_operations, 3);
        assert!(a.reasons.contains(&"CPU_CAPACITY".into()));
    }

    #[test]
    fn constrained_memory_reduces_but_does_not_stop_progress() {
        let p = ResourcePolicy {
            reserved_memory_bytes: 0,
            ..ResourcePolicy::default()
        };
        let a = advise(&snapshot(4_000, Some(8_700), Some(1)), &p).unwrap();
        assert_eq!(a.pressure, ResourcePressure::Constrained);
        assert_eq!(a.recommended_parallel_operations, 4);
        assert!(!a.defer_nonessential_new_work);
    }

    #[test]
    fn critical_memory_preserves_one_path_of_progress() {
        let p = ResourcePolicy::default();
        let a = advise(&snapshot(8_000, Some(9_700), Some(u64::MAX)), &p).unwrap();
        assert_eq!(a.pressure, ResourcePressure::Critical);
        assert_eq!(a.recommended_parallel_operations, 1);
        assert!(a.defer_nonessential_new_work);
    }

    #[test]
    fn finite_pid_headroom_caps_parallel_advice_without_guessing_memory_pressure() {
        let p = ResourcePolicy {
            reserved_memory_bytes: 0,
            ..ResourcePolicy::default()
        };
        let mut s = snapshot(8_000, Some(2_000), Some(u64::MAX));
        s.pids_current = Some(14);
        s.pids_limit = Some(16);
        s.pids_limit_observed = true;
        s.pids_limit_unlimited = false;
        let a = advise(&s, &p).unwrap();
        assert_eq!(a.pressure, ResourcePressure::Normal);
        assert_eq!(a.observed_parallel_ceiling, 2);
        assert_eq!(a.recommended_parallel_operations, 2);
        assert!(a.reasons.contains(&"PID_CAPACITY".into()));
        assert!(!a.defer_nonessential_new_work);
    }

    #[test]
    fn exhausted_pid_limit_is_critical_operational_pressure() {
        let p = ResourcePolicy {
            reserved_memory_bytes: 0,
            ..ResourcePolicy::default()
        };
        let mut s = snapshot(8_000, Some(2_000), Some(u64::MAX));
        s.pids_current = Some(16);
        s.pids_limit = Some(16);
        s.pids_limit_observed = true;
        s.pids_limit_unlimited = false;
        let a = advise(&s, &p).unwrap();
        assert_eq!(a.pressure, ResourcePressure::Critical);
        assert_eq!(a.recommended_parallel_operations, 1);
        assert!(a.reasons.contains(&"PID_LIMIT_EXHAUSTED".into()));
        assert!(a.defer_nonessential_new_work);
    }

    #[test]
    fn delegated_mode_observes_but_does_not_authorize_auto_tuning() {
        let p = ResourcePolicy {
            mode: ResourceManagementMode::ExternallyManaged,
            ..ResourcePolicy::default()
        };
        let a = advise(&snapshot(1_000, Some(9_700), Some(0)), &p).unwrap();
        assert!(!a.apply_concurrency_automatically);
        assert_eq!(a.observed_parallel_ceiling, 2);
        assert_eq!(a.recommended_parallel_operations, 1);
        assert!(a.reasons.contains(&"OPERATOR_DELEGATED_CONTROL".into()));
    }

    #[test]
    fn unknown_memory_does_not_invent_a_pressure_measurement() {
        let p = ResourcePolicy::default();
        let a = advise(&snapshot(2_000, None, None), &p).unwrap();
        assert_eq!(a.pressure, ResourcePressure::Unknown);
        assert!(a.reasons.contains(&"MEMORY_PRESSURE_UNKNOWN".into()));
    }

    #[test]
    fn invalid_thresholds_and_zero_budget_fail_closed() {
        let p = ResourcePolicy {
            operator_max_parallel_operations: 0,
            ..ResourcePolicy::default()
        };
        assert_eq!(
            advise(&snapshot(1_000, None, None), &p),
            Err(ResourcePolicyError::Invalid)
        );

        let p = ResourcePolicy {
            high_memory_pressure_basis_points: 9_500,
            critical_memory_pressure_basis_points: 9_000,
            ..ResourcePolicy::default()
        };
        assert!(validate(&p).is_err());
    }
}
