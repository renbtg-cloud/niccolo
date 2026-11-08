//! Pure adaptive-resource policy advice.
//!
//! ARM-002 deliberately separates observation, policy and enforcement. This
//! module consumes a ResourceSnapshot and returns transparent recommendations.
//! It does not mutate cgroups, queues, model selection, concurrency, or the
//! epistemic state of an investigation.
use crate::runtime_resources::ResourceSnapshot;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourceManagementMode {
    SelfManaged,
    ExternallyManaged,
    Hybrid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourcePressure {
    Unknown,
    Normal,
    Elevated,
    Critical,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveResourcePolicy {
    pub mode: ResourceManagementMode,
    /// Local upper bound even if the effective CPU envelope is larger.
    pub max_local_parallelism: u16,
    /// 8500 means 85% of a finite cgroup memory limit.
    pub memory_soft_pressure_basis_points: u16,
    pub memory_hard_pressure_basis_points: u16,
    /// A finite cgroup with less remaining memory than this is treated as
    /// critical for new memory-intensive work, regardless of percentage.
    pub min_memory_headroom_bytes: u64,
    pub pids_soft_pressure_basis_points: u16,
    pub pids_hard_pressure_basis_points: u16,
}

impl Default for AdaptiveResourcePolicy {
    fn default() -> Self {
        Self {
            mode: ResourceManagementMode::SelfManaged,
            max_local_parallelism: 32,
            memory_soft_pressure_basis_points: 8500,
            memory_hard_pressure_basis_points: 9500,
            min_memory_headroom_bytes: 256 * 1024 * 1024,
            pids_soft_pressure_basis_points: 8500,
            pids_hard_pressure_basis_points: 9500,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAdvice {
    pub mode: ResourceManagementMode,
    pub observed_pressure: ResourcePressure,
    /// None means Niccolo deliberately delegates local concurrency choice to
    /// an external manager. Some(value) remains advice until enforcement is
    /// explicitly wired into the runtime.
    pub recommended_max_parallelism: Option<u16>,
    /// None means no local admission recommendation in externally managed mode.
    pub recommend_admit_new_expensive_work: Option<bool>,
    pub reasons: Vec<String>,
    pub advice_is_heuristic: bool,
    pub advice_is_enforced: bool,
    pub mutates_resource_limits: bool,
    pub establishes_epistemic_quality: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResourcePolicyError {
    #[error("invalid adaptive resource policy")]
    InvalidPolicy,
    #[error("invalid resource snapshot")]
    InvalidSnapshot,
}

pub fn valid(policy: &AdaptiveResourcePolicy) -> bool {
    policy.max_local_parallelism > 0
        && policy.max_local_parallelism <= 1024
        && policy.memory_soft_pressure_basis_points < policy.memory_hard_pressure_basis_points
        && policy.memory_hard_pressure_basis_points <= 10_000
        && policy.pids_soft_pressure_basis_points < policy.pids_hard_pressure_basis_points
        && policy.pids_hard_pressure_basis_points <= 10_000
        && policy.min_memory_headroom_bytes <= 16 * 1024 * 1024 * 1024 * 1024
}

fn ratio_basis_points(current: u64, limit: u64) -> Result<u16, ResourcePolicyError> {
    if limit == 0 {
        return Err(ResourcePolicyError::InvalidSnapshot);
    }
    let points = (u128::from(current) * 10_000 / u128::from(limit)).min(10_000);
    u16::try_from(points).map_err(|_| ResourcePolicyError::InvalidSnapshot)
}

fn upgrade(current: &mut ResourcePressure, candidate: ResourcePressure) {
    if candidate > *current {
        *current = candidate;
    }
}

fn cpu_parallelism_ceiling(
    snapshot: &ResourceSnapshot,
    policy: &AdaptiveResourcePolicy,
) -> Result<u16, ResourcePolicyError> {
    if snapshot.effective_cpu_capacity_millicores == 0 {
        return Err(ResourcePolicyError::InvalidSnapshot);
    }
    let full_cores = (snapshot.effective_cpu_capacity_millicores / 1000).max(1);
    let full_cores = u16::try_from(full_cores.min(u64::from(u16::MAX)))
        .map_err(|_| ResourcePolicyError::InvalidSnapshot)?;
    Ok(full_cores.min(policy.max_local_parallelism).max(1))
}

/// Return resource-management advice without changing runtime state.
///
/// Memory and PID percentages are scheduling heuristics, not reliability or
/// epistemic-quality estimates. CPU quota/cpuset are capacity ceilings, not
/// current utilization. Historical throttling counters are intentionally not
/// interpreted as present load without a time-delta sample.
pub fn advise(
    snapshot: &ResourceSnapshot,
    policy: &AdaptiveResourcePolicy,
) -> Result<ResourceAdvice, ResourcePolicyError> {
    if !valid(policy) {
        return Err(ResourcePolicyError::InvalidPolicy);
    }
    if snapshot.logical_parallelism == 0
        || snapshot.effective_cpu_capacity_millicores == 0
        || snapshot.memory_pressure_basis_points.is_some_and(|value| value > 10_000)
    {
        return Err(ResourcePolicyError::InvalidSnapshot);
    }

    let mut pressure = ResourcePressure::Unknown;
    let mut reasons = Vec::new();
    let mut any_pressure_signal = false;

    if let Some(memory_pressure) = snapshot.memory_pressure_basis_points {
        any_pressure_signal = true;
        upgrade(&mut pressure, ResourcePressure::Normal);
        if memory_pressure >= policy.memory_hard_pressure_basis_points {
            upgrade(&mut pressure, ResourcePressure::Critical);
            reasons.push("MEMORY_CRITICAL".into());
        } else if memory_pressure >= policy.memory_soft_pressure_basis_points {
            upgrade(&mut pressure, ResourcePressure::Elevated);
            reasons.push("MEMORY_ELEVATED".into());
        }
    }

    if let (Some(headroom), Some(_limit)) =
        (snapshot.memory_headroom_bytes, snapshot.memory_limit_bytes)
    {
        any_pressure_signal = true;
        upgrade(&mut pressure, ResourcePressure::Normal);
        if headroom < policy.min_memory_headroom_bytes {
            upgrade(&mut pressure, ResourcePressure::Critical);
            reasons.push("MEMORY_HEADROOM_LOW".into());
        }
    }

    if let (Some(current), Some(limit)) = (snapshot.pids_current, snapshot.pids_limit) {
        any_pressure_signal = true;
        let pids_pressure = ratio_basis_points(current, limit)?;
        upgrade(&mut pressure, ResourcePressure::Normal);
        if pids_pressure >= policy.pids_hard_pressure_basis_points {
            upgrade(&mut pressure, ResourcePressure::Critical);
            reasons.push("PIDS_CRITICAL".into());
        } else if pids_pressure >= policy.pids_soft_pressure_basis_points {
            upgrade(&mut pressure, ResourcePressure::Elevated);
            reasons.push("PIDS_ELEVATED".into());
        }
    }

    if !any_pressure_signal {
        pressure = ResourcePressure::Unknown;
        reasons.push("RESOURCE_PRESSURE_UNKNOWN".into());
    }

    let cpu_ceiling = cpu_parallelism_ceiling(snapshot, policy)?;
    let (recommended_max_parallelism, recommend_admit_new_expensive_work) = match policy.mode {
        ResourceManagementMode::ExternallyManaged => {
            reasons.push("EXTERNAL_MANAGEMENT_DELEGATED".into());
            (None, None)
        }
        ResourceManagementMode::SelfManaged | ResourceManagementMode::Hybrid => {
            let recommended = match pressure {
                ResourcePressure::Critical => 1,
                ResourcePressure::Elevated => (cpu_ceiling / 2).max(1),
                ResourcePressure::Normal | ResourcePressure::Unknown => cpu_ceiling,
            };
            if recommended < cpu_ceiling {
                reasons.push("LOCAL_PARALLELISM_REDUCTION_RECOMMENDED".into());
            } else {
                reasons.push("LOCAL_PARALLELISM_CEILING".into());
            }
            (
                Some(recommended),
                Some(pressure != ResourcePressure::Critical),
            )
        }
    };

    Ok(ResourceAdvice {
        mode: policy.mode,
        observed_pressure: pressure,
        recommended_max_parallelism,
        recommend_admit_new_expensive_work,
        reasons,
        advice_is_heuristic: true,
        advice_is_enforced: false,
        mutates_resource_limits: false,
        establishes_epistemic_quality: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> ResourceSnapshot {
        ResourceSnapshot {
            platform: "linux".into(),
            logical_parallelism: 8,
            process_rss_bytes: Some(512 * 1024 * 1024),
            process_peak_rss_bytes: Some(768 * 1024 * 1024),
            cgroup_v2: true,
            cgroup_path: Some("/niccolo".into()),
            memory_current_bytes: Some(2 * 1024 * 1024 * 1024),
            memory_limit_bytes: Some(4 * 1024 * 1024 * 1024),
            memory_limit_observed: true,
            memory_limit_unlimited: false,
            memory_headroom_bytes: Some(2 * 1024 * 1024 * 1024),
            memory_pressure_basis_points: Some(5000),
            cpu_quota_millicores: Some(2500),
            cpu_quota_observed: true,
            cpu_quota_unlimited: false,
            cpuset_cpu_count: Some(4),
            effective_cpu_capacity_millicores: 2500,
            cpu_throttled_events: Some(100),
            cpu_throttled_usec: Some(500_000),
            pids_current: Some(10),
            pids_limit: Some(100),
            pids_limit_observed: true,
            pids_limit_unlimited: false,
            gpu_observation_implemented: false,
            mutates_resource_limits: false,
            establishes_epistemic_quality: false,
        }
    }

    #[test]
    fn normal_self_managed_mode_respects_effective_cpu_capacity() {
        let result = advise(&snapshot(), &AdaptiveResourcePolicy::default()).unwrap();
        assert_eq!(result.observed_pressure, ResourcePressure::Normal);
        assert_eq!(result.recommended_max_parallelism, Some(2));
        assert_eq!(result.recommend_admit_new_expensive_work, Some(true));
        assert!(result.reasons.contains(&"LOCAL_PARALLELISM_CEILING".into()));
        assert!(!result.advice_is_enforced);
        assert!(!result.mutates_resource_limits);
        assert!(!result.establishes_epistemic_quality);
    }

    #[test]
    fn elevated_memory_recommends_reduced_parallelism_without_claiming_quality() {
        let mut input = snapshot();
        input.memory_pressure_basis_points = Some(9000);
        input.memory_current_bytes = Some(3600);
        input.memory_limit_bytes = Some(4000);
        input.memory_headroom_bytes = Some(400);
        let mut policy = AdaptiveResourcePolicy::default();
        policy.min_memory_headroom_bytes = 100;
        policy.max_local_parallelism = 8;
        input.effective_cpu_capacity_millicores = 4000;
        let result = advise(&input, &policy).unwrap();
        assert_eq!(result.observed_pressure, ResourcePressure::Elevated);
        assert_eq!(result.recommended_max_parallelism, Some(2));
        assert_eq!(result.recommend_admit_new_expensive_work, Some(true));
        assert!(result.reasons.contains(&"MEMORY_ELEVATED".into()));
    }

    #[test]
    fn critical_memory_recommends_one_worker_and_no_new_expensive_work() {
        let mut input = snapshot();
        input.memory_pressure_basis_points = Some(9700);
        input.memory_headroom_bytes = Some(64 * 1024 * 1024);
        let result = advise(&input, &AdaptiveResourcePolicy::default()).unwrap();
        assert_eq!(result.observed_pressure, ResourcePressure::Critical);
        assert_eq!(result.recommended_max_parallelism, Some(1));
        assert_eq!(result.recommend_admit_new_expensive_work, Some(false));
        assert!(result.reasons.contains(&"MEMORY_CRITICAL".into()));
        assert!(result.reasons.contains(&"MEMORY_HEADROOM_LOW".into()));
    }

    #[test]
    fn pid_pressure_is_independent_from_memory_pressure() {
        let mut input = snapshot();
        input.memory_pressure_basis_points = None;
        input.memory_headroom_bytes = None;
        input.memory_limit_bytes = None;
        input.pids_current = Some(96);
        input.pids_limit = Some(100);
        let result = advise(&input, &AdaptiveResourcePolicy::default()).unwrap();
        assert_eq!(result.observed_pressure, ResourcePressure::Critical);
        assert!(result.reasons.contains(&"PIDS_CRITICAL".into()));
    }

    #[test]
    fn externally_managed_mode_observes_pressure_but_does_not_prescribe_local_actions() {
        let mut input = snapshot();
        input.memory_pressure_basis_points = Some(9900);
        let mut policy = AdaptiveResourcePolicy::default();
        policy.mode = ResourceManagementMode::ExternallyManaged;
        let result = advise(&input, &policy).unwrap();
        assert_eq!(result.observed_pressure, ResourcePressure::Critical);
        assert_eq!(result.recommended_max_parallelism, None);
        assert_eq!(result.recommend_admit_new_expensive_work, None);
        assert!(result.reasons.contains(&"EXTERNAL_MANAGEMENT_DELEGATED".into()));
    }

    #[test]
    fn unknown_pressure_does_not_fabricate_memory_or_pid_state() {
        let mut input = snapshot();
        input.memory_pressure_basis_points = None;
        input.memory_headroom_bytes = None;
        input.memory_limit_bytes = None;
        input.pids_current = None;
        input.pids_limit = None;
        let result = advise(&input, &AdaptiveResourcePolicy::default()).unwrap();
        assert_eq!(result.observed_pressure, ResourcePressure::Unknown);
        assert_eq!(result.recommended_max_parallelism, Some(2));
        assert_eq!(result.recommend_admit_new_expensive_work, Some(true));
        assert!(result.reasons.contains(&"RESOURCE_PRESSURE_UNKNOWN".into()));
    }

    #[test]
    fn invalid_policy_thresholds_and_zero_capacity_fail_closed() {
        let mut policy = AdaptiveResourcePolicy::default();
        policy.memory_soft_pressure_basis_points = 9600;
        policy.memory_hard_pressure_basis_points = 9500;
        assert_eq!(
            advise(&snapshot(), &policy),
            Err(ResourcePolicyError::InvalidPolicy)
        );

        let mut input = snapshot();
        input.effective_cpu_capacity_millicores = 0;
        assert_eq!(
            advise(&input, &AdaptiveResourcePolicy::default()),
            Err(ResourcePolicyError::InvalidSnapshot)
        );
    }

    #[test]
    fn cumulative_throttling_is_not_misread_as_current_cpu_pressure() {
        let mut input = snapshot();
        input.cpu_throttled_events = Some(u64::MAX);
        input.cpu_throttled_usec = Some(u64::MAX);
        let result = advise(&input, &AdaptiveResourcePolicy::default()).unwrap();
        assert_eq!(result.observed_pressure, ResourcePressure::Normal);
        assert_eq!(result.recommended_max_parallelism, Some(2));
    }
}
