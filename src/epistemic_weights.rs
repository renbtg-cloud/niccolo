//! Explainable, conservative evidence-weight accounting.
//!
//! This is NOT Bayesian inference, calibrated truth probability, or evidence verification.
//! It cannot establish witness/source independence. It never promotes a user's
//! subjective confidence into evidence or a fact.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Stance {
    Supports,
    Contradicts,
    ContextOnly,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightedObservation {
    pub evidence_id: Uuid,
    /// Known derivation root; distinct roots are NOT a declaration of independence.
    /// All missing roots share one conservative bucket.
    pub documented_origin_id: Option<Uuid>,
    pub stance: Stance,
    /// Explicit analyst/user weight for this item, never a calibrated probability.
    pub declared_strength: u16,
    pub rationale: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightPolicy {
    /// Multiplier in percent (100 = neutral, 0 = ignore). Range 0..=200.
    pub support_multiplier_pct: u16,
    pub counter_multiplier_pct: u16,
    /// Maximum contribution of one derivation root to each direction; 0..=100.
    pub per_origin_cap: u16,
}

impl Default for WeightPolicy {
    fn default() -> Self {
        Self {
            support_multiplier_pct: 100,
            counter_multiplier_pct: 100,
            per_origin_cap: 100,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OriginContribution {
    pub documented_origin_id: Option<Uuid>,
    pub item_count: usize,
    pub max_support_strength: u16,
    pub max_counter_strength: u16,
    pub weighted_support: u16,
    pub weighted_counter: u16,
    pub contains_conflicting_signals: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BalanceReport {
    pub support_points: i32,
    pub counter_points: i32,
    pub net_points: i32,
    pub input_item_count: usize,
    pub origins: Vec<OriginContribution>,
    pub human_declared_confidence_percent: Option<u8>,
    pub score_is_probability_of_truth: bool,
    pub independence_established: bool,
    pub human_confidence_is_documentary_evidence: bool,
    pub establishes_fact: bool,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum WeightError {
    #[error("invalid weight policy or observation weight")]
    InvalidWeight,
    #[error("empty, overlong or invalid rationale")]
    InvalidRationale,
    #[error("duplicate evidence ID")]
    DuplicateEvidence,
    #[error("too many observations")]
    TooManyObservations,
}

fn weighted(strength: u16, factor: u16, cap: u16) -> u16 {
    ((u32::from(strength) * u32::from(factor)) / 100).min(u32::from(cap)) as u16
}

/// A transparent *heuristic balance*, not a scientific estimate of truth.
///
/// - Repeated evidence from one documented root contributes at most its maximum
///   supporting strength and maximum opposing strength, never a sum of copies.
/// - Evidence lacking a known root is grouped conservatively into one bucket.
/// - A separate 90% human opinion remains a 90% human opinion: it does not
///   alter the evidence balance unless a later explicit policy says so.
/// - Different roots are NOT assumed statistically independent.
pub fn evaluate(
    policy: WeightPolicy,
    human_declared_confidence_percent: Option<u8>,
    observations: &[WeightedObservation],
) -> Result<BalanceReport, WeightError> {
    if policy.support_multiplier_pct > 200
        || policy.counter_multiplier_pct > 200
        || policy.per_origin_cap > 100
        || human_declared_confidence_percent.is_some_and(|value| value > 100)
    {
        return Err(WeightError::InvalidWeight);
    }
    if observations.len() > 512 {
        return Err(WeightError::TooManyObservations);
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut by_origin: BTreeMap<Option<Uuid>, (usize, u16, u16)> = BTreeMap::new();
    for o in observations {
        if o.declared_strength > 100 {
            return Err(WeightError::InvalidWeight);
        }
        // Rationale is an inspectable human/analyst explanation. Reject
        // invisible direction overrides and terminal-control payloads that
        // could alter how the reported evidence balance is perceived.
        // Ordinary multi-line paragraphs, tabs and Unicode text remain valid.
        if o.rationale.trim().is_empty()
            || o.rationale.len() > 4096
            || o.rationale.chars().any(|ch| {
                (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
                    || matches!(
                        ch,
                        '\u{061c}'
                            | '\u{200e}'
                            | '\u{200f}'
                            | '\u{202a}'..='\u{202e}'
                            | '\u{2066}'..='\u{2069}'
                    )
            })
        {
            return Err(WeightError::InvalidRationale);
        }
        if !seen.insert(o.evidence_id) {
            return Err(WeightError::DuplicateEvidence);
        }
        let bucket = by_origin.entry(o.documented_origin_id).or_default();
        bucket.0 += 1;
        match o.stance {
            Stance::Supports => bucket.1 = bucket.1.max(o.declared_strength),
            Stance::Contradicts => bucket.2 = bucket.2.max(o.declared_strength),
            Stance::ContextOnly => (),
        }
    }

    let mut origins = Vec::with_capacity(by_origin.len());
    let mut support_points = 0i32;
    let mut counter_points = 0i32;
    for (origin, (item_count, support, counter)) in by_origin {
        let weighted_support = weighted(
            support,
            policy.support_multiplier_pct,
            policy.per_origin_cap,
        );
        let weighted_counter = weighted(
            counter,
            policy.counter_multiplier_pct,
            policy.per_origin_cap,
        );
        support_points += i32::from(weighted_support);
        counter_points += i32::from(weighted_counter);
        origins.push(OriginContribution {
            documented_origin_id: origin,
            item_count,
            max_support_strength: support,
            max_counter_strength: counter,
            weighted_support,
            weighted_counter,
            contains_conflicting_signals: support > 0 && counter > 0,
        });
    }
    Ok(BalanceReport {
        support_points,
        counter_points,
        net_points: support_points - counter_points,
        input_item_count: observations.len(),
        origins,
        human_declared_confidence_percent,
        score_is_probability_of_truth: false,
        independence_established: false,
        human_confidence_is_documentary_evidence: false,
        establishes_fact: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn obs(
        evidence: u128,
        origin: Option<u128>,
        stance: Stance,
        strength: u16,
    ) -> WeightedObservation {
        WeightedObservation {
            evidence_id: Uuid::from_u128(evidence),
            documented_origin_id: origin.map(Uuid::from_u128),
            stance,
            declared_strength: strength,
            rationale: "explicitly assigned by the analyst".into(),
        }
    }
    #[test]
    fn three_copies_do_not_triple_support() {
        let rows = (1..=3)
            .map(|i| obs(i, Some(10), Stance::Supports, 80))
            .collect::<Vec<_>>();
        let result = evaluate(WeightPolicy::default(), None, &rows).unwrap();
        assert_eq!(result.support_points, 80);
        assert_eq!(result.origins.len(), 1);
        assert_eq!(result.origins[0].item_count, 3);
        assert!(!result.independence_established);
    }
    #[test]
    fn missing_origins_are_not_assumed_independent() {
        let rows = vec![
            obs(1, None, Stance::Supports, 70),
            obs(2, None, Stance::Supports, 70),
        ];
        assert_eq!(
            evaluate(WeightPolicy::default(), None, &rows)
                .unwrap()
                .support_points,
            70
        );
    }
    #[test]
    fn opposing_evidence_remains_visible() {
        let rows = vec![
            obs(1, Some(5), Stance::Supports, 80),
            obs(2, Some(6), Stance::Contradicts, 60),
        ];
        let result = evaluate(WeightPolicy::default(), None, &rows).unwrap();
        assert_eq!(
            (
                result.support_points,
                result.counter_points,
                result.net_points
            ),
            (80, 60, 20)
        );
        assert!(!result.establishes_fact);
    }
    #[test]
    fn conflicting_signals_from_same_origin_stay_disclosed() {
        let rows = vec![
            obs(1, Some(5), Stance::Supports, 75),
            obs(2, Some(5), Stance::Contradicts, 85),
        ];
        let result = evaluate(WeightPolicy::default(), None, &rows).unwrap();
        assert_eq!(result.net_points, -10);
        assert!(result.origins[0].contains_conflicting_signals);
    }
    #[test]
    fn invalid_human_confidence_cannot_exceed_one_hundred() {
        let rows = vec![obs(1, Some(5), Stance::Supports, 80)];
        assert_eq!(
            evaluate(WeightPolicy::default(), Some(101), &rows),
            Err(WeightError::InvalidWeight)
        );
    }
    #[test]
    fn human_ninety_percent_is_not_a_score_boost() {
        let rows = vec![obs(1, Some(5), Stance::Supports, 10)];
        let a = evaluate(WeightPolicy::default(), None, &rows).unwrap();
        let b = evaluate(WeightPolicy::default(), Some(90), &rows).unwrap();
        assert_eq!(a.net_points, b.net_points);
        assert_eq!(b.human_declared_confidence_percent, Some(90));
        assert!(!b.human_confidence_is_documentary_evidence);
        assert!(!b.score_is_probability_of_truth);
    }
    #[test]
    fn configurable_multiplier_is_deterministic() {
        let rows = vec![
            obs(1, Some(5), Stance::Supports, 80),
            obs(2, Some(6), Stance::Contradicts, 40),
        ];
        let p = WeightPolicy {
            support_multiplier_pct: 50,
            counter_multiplier_pct: 200,
            per_origin_cap: 100,
        };
        assert_eq!(evaluate(p, None, &rows).unwrap().net_points, -40);
    }
    #[test]
    fn weaker_copies_from_a_known_origin_cannot_manufacture_corroboration() {
        let baseline = vec![
            obs(1, Some(5), Stance::Supports, 80),
            obs(2, Some(5), Stance::Contradicts, 65),
        ];
        let original = evaluate(WeightPolicy::default(), None, &baseline).unwrap();
        let mut copied = baseline;
        for (id, stance, strength) in [
            (3, Stance::Supports, 70),
            (4, Stance::Supports, 70),
            (5, Stance::Contradicts, 60),
            (6, Stance::ContextOnly, 100),
        ] {
            copied.push(obs(id, Some(5), stance, strength));
        }
        let report = evaluate(WeightPolicy::default(), Some(99), &copied).unwrap();
        assert_eq!(report.support_points, original.support_points);
        assert_eq!(report.counter_points, original.counter_points);
        assert_eq!(report.net_points, original.net_points);
        assert_eq!(report.origins[0].item_count, 6);
        assert!(!report.independence_established);
        assert!(!report.human_confidence_is_documentary_evidence);
        assert!(!report.establishes_fact);
    }

    #[test]
    fn evidence_input_permutation_cannot_change_explainable_balance() {
        let rows = vec![
            obs(1, Some(17), Stance::Supports, 87),
            obs(2, Some(17), Stance::Contradicts, 62),
            obs(3, None, Stance::Supports, 50),
            obs(4, None, Stance::ContextOnly, 90),
            obs(5, Some(31), Stance::Contradicts, 33),
        ];
        let policy = WeightPolicy {
            support_multiplier_pct: 140,
            counter_multiplier_pct: 75,
            per_origin_cap: 85,
        };
        let reference = evaluate(policy, Some(23), &rows).unwrap();
        for offset in 0..rows.len() {
            let mut permutation = rows.clone();
            permutation.rotate_left(offset);
            if offset % 2 == 1 {
                permutation.reverse();
            }
            let score = evaluate(policy, Some(23), &permutation).unwrap();
            assert_eq!(
                score, reference,
                "input ordering changed the score at rotation {offset}"
            );
        }
        assert!(!reference.score_is_probability_of_truth);
        assert!(!reference.independence_established);
    }

    #[test]
    fn context_is_not_evidence() {
        let rows = vec![obs(1, Some(5), Stance::ContextOnly, 100)];
        assert_eq!(
            evaluate(WeightPolicy::default(), None, &rows)
                .unwrap()
                .net_points,
            0
        );
    }
    #[test]
    fn evidence_rationales_remain_readable_and_cannot_hide_directional_controls() {
        let mut row = obs(1, Some(5), Stance::Supports, 40);
        for bad in [
            "claim supported\u{202e}DENIED",
            "claim supported\u{2066}DENIED",
            "claim supported\u{001b}[2J",
            "claim supported\u{0000}hidden",
        ] {
            row.rationale = bad.to_owned();
            assert_eq!(
                evaluate(WeightPolicy::default(), None, &[row.clone()]),
                Err(WeightError::InvalidRationale)
            );
        }
        row.rationale =
            "Reason 1: observed by Ana.\nReason 2: not independently checked.\tRevisit.".into();
        let report = evaluate(WeightPolicy::default(), None, &[row]).unwrap();
        assert_eq!(report.support_points, 40);
        assert!(!report.independence_established);
        assert!(!report.establishes_fact);
    }

    #[test]
    fn invalid_and_duplicate_inputs_fail_closed() {
        assert_eq!(
            evaluate(
                WeightPolicy::default(),
                None,
                &[obs(1, Some(5), Stance::Supports, 101)]
            ),
            Err(WeightError::InvalidWeight)
        );
        let row = obs(1, Some(5), Stance::Supports, 10);
        assert_eq!(
            evaluate(WeightPolicy::default(), None, &[row.clone(), row]),
            Err(WeightError::DuplicateEvidence)
        );
    }
}
