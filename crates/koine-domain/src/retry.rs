//! Retry policy: deterministic exponential backoff with full jitter (spec §3).
//!
//! Pure function of (policy, attempt, seed) — the seed comes from the
//! application's `IdGenerator` port so the domain stays deterministic.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// What happens after a failed attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryDecision {
    /// Retry after the given delay.
    RetryAfter(Duration),
    /// Attempts exhausted — park the job.
    Park,
}

/// Exponential backoff with full jitter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum failed attempts before parking (attempts are 1-based).
    pub max_attempts: u32,
    /// Backoff base: the cap for the first retry's delay.
    pub base_delay: Duration,
    /// Upper cap for any computed delay.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 20,
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_mins(15),
        }
    }
}

impl RetryPolicy {
    /// Decision after `attempts_completed` failed attempts (≥1).
    ///
    /// Full jitter: the delay is uniform in `[0, min(base * 2^(n-1), cap)]`,
    /// driven entirely by `seed` — equal inputs give equal outputs.
    #[must_use]
    pub fn decide(&self, attempts_completed: u32, seed: u64) -> RetryDecision {
        if attempts_completed >= self.max_attempts {
            return RetryDecision::Park;
        }
        let exp = attempts_completed.saturating_sub(1).min(31);
        let uncapped = self.base_delay.saturating_mul(2_u32.saturating_pow(exp));
        let capped = uncapped.min(self.max_delay);
        let millis = u64::try_from(capped.as_millis())
            .unwrap_or(u64::MAX)
            .min(u64::MAX - 1);
        let jittered = if millis == 0 {
            0
        } else {
            splitmix64(seed ^ u64::from(attempts_completed)) % (millis + 1)
        };
        RetryDecision::RetryAfter(Duration::from_millis(jittered))
    }
}

/// `SplitMix64` — tiny, well-distributed, dependency-free PRNG step.
fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn parks_when_attempts_exhausted() {
        let p = RetryPolicy {
            max_attempts: 3,
            ..RetryPolicy::default()
        };
        assert_eq!(p.decide(3, 42), RetryDecision::Park);
        assert_eq!(p.decide(4, 42), RetryDecision::Park);
    }

    #[test]
    fn is_deterministic_for_equal_inputs() {
        let p = RetryPolicy::default();
        assert_eq!(p.decide(2, 1234), p.decide(2, 1234));
    }

    #[test]
    fn different_seeds_can_differ() {
        let p = RetryPolicy::default();
        let outcomes: std::collections::HashSet<_> = (0..32u64)
            .map(|s| format!("{:?}", p.decide(5, s)))
            .collect();
        assert!(outcomes.len() > 1, "jitter must actually vary");
    }

    #[test]
    fn delay_never_exceeds_cap() {
        let p = RetryPolicy {
            max_attempts: 100,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
        };
        for attempt in 1..99 {
            for seed in 0..16u64 {
                if let RetryDecision::RetryAfter(d) = p.decide(attempt, seed) {
                    assert!(
                        d <= Duration::from_secs(30),
                        "attempt {attempt} seed {seed}: {d:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn extreme_max_delay_never_panics() {
        let p = RetryPolicy {
            max_attempts: 10,
            base_delay: Duration::MAX,
            max_delay: Duration::MAX,
        };
        assert!(matches!(p.decide(1, 7), RetryDecision::RetryAfter(_)));
    }

    #[test]
    fn first_attempt_delay_is_within_base() {
        let p = RetryPolicy::default();
        if let RetryDecision::RetryAfter(d) = p.decide(1, 99) {
            assert!(d <= p.base_delay);
        } else {
            panic!("attempt 1 of 20 must retry");
        }
    }

    #[test]
    fn different_attempts_can_differ_for_fixed_seed() {
        let p = RetryPolicy::default();
        let outcomes: std::collections::HashSet<_> = (1..16u32)
            .map(|attempt| format!("{:?}", p.decide(attempt, 42)))
            .collect();
        assert!(
            outcomes.len() > 4,
            "fixed-seed delays must vary across attempts"
        );
    }
}
