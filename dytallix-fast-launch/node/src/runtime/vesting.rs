use crate::storage::state::Storage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A single vesting/lockup schedule for one address+denom.
///
/// Locked tokens physically remain in the account balance; this schedule is
/// consulted at every balance-sufficiency check to compute the *spendable*
/// portion (`total - locked(height)`). Because `locked(h)` is monotonically
/// non-increasing, a balance that passes a sufficiency check at height `h` will
/// still satisfy `locked(h')` for any `h' > h`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VestingSchedule {
    pub denom: String,
    pub total_locked: u128,
    pub start_height: u64,
    pub cliff_height: u64,
    pub end_height: u64,
}

impl VestingSchedule {
    /// Amount released (unlocked) at height `h`.
    pub fn released(&self, h: u64) -> u128 {
        if h < self.cliff_height {
            return 0;
        }
        if h >= self.end_height {
            return self.total_locked;
        }
        let span = (self.end_height - self.start_height) as u128;
        if span == 0 {
            return self.total_locked;
        }
        let elapsed = h.saturating_sub(self.start_height) as u128;
        self.total_locked
            .checked_mul(elapsed)
            .map(|p| p / span)
            .unwrap_or(self.total_locked)
    }

    /// Amount still locked at height `h`.
    pub fn locked(&self, h: u64) -> u128 {
        self.total_locked.saturating_sub(self.released(h))
    }
}

/// Registry of vesting schedules, persisted in the same RocksDB instance as the
/// rest of chain state. Mirrors the shape/threading of `StakingModule`.
pub struct VestingModule {
    storage: Arc<Storage>,
}

impl VestingModule {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self { storage }
    }

    fn key(addr: &str) -> String {
        format!("vesting:schedule:{addr}")
    }

    pub fn register_schedule(&self, addr: &str, s: &VestingSchedule) {
        if let Ok(b) = bincode::serialize(s) {
            let _ = self.storage.db.put(Self::key(addr), b);
        }
    }

    pub fn load_schedule(&self, addr: &str) -> Option<VestingSchedule> {
        self.storage
            .db
            .get(Self::key(addr))
            .ok()
            .flatten()
            .and_then(|b| bincode::deserialize::<VestingSchedule>(&b).ok())
    }

    /// Locked amount for `addr` in `denom` at height `h` (0 if no schedule or
    /// the schedule is for a different denom).
    pub fn locked_amount(&self, addr: &str, denom: &str, h: u64) -> u128 {
        match self.load_schedule(addr) {
            Some(s) if s.denom == denom => s.locked(h),
            _ => 0,
        }
    }

    /// Spendable portion of `total` for `addr` in `denom` at height `h`.
    pub fn spendable(&self, total: u128, addr: &str, denom: &str, h: u64) -> u128 {
        total.saturating_sub(self.locked_amount(addr, denom, h))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sched(total: u128, start: u64, cliff: u64, end: u64) -> VestingSchedule {
        VestingSchedule {
            denom: "udgt".to_string(),
            total_locked: total,
            start_height: start,
            cliff_height: cliff,
            end_height: end,
        }
    }

    #[test]
    fn pre_cliff_releases_nothing() {
        let s = sched(1000, 0, 100, 1000);
        assert_eq!(s.released(0), 0);
        assert_eq!(s.released(99), 0);
        assert_eq!(s.locked(99), 1000);
    }

    #[test]
    fn at_and_after_end_releases_full() {
        let s = sched(1000, 0, 100, 1000);
        assert_eq!(s.released(1000), 1000);
        assert_eq!(s.released(5000), 1000);
        assert_eq!(s.locked(1000), 0);
        assert_eq!(s.locked(5000), 0);
    }

    #[test]
    fn linear_release_between_cliff_and_end() {
        let s = sched(1000, 0, 100, 1000);
        // floor(total * (h - start) / (end - start))
        assert_eq!(s.released(100), 100); // 1000 * 100 / 1000
        assert_eq!(s.released(500), 500);
        assert_eq!(s.released(999), 999);
        assert_eq!(s.locked(500), 500);
    }

    #[test]
    fn floor_division_is_exact() {
        // total=7, span=3 -> at h=1 released=floor(7*1/3)=2
        let s = sched(7, 0, 0, 3);
        assert_eq!(s.released(1), 2);
        assert_eq!(s.released(2), 4);
        assert_eq!(s.released(3), 7);
    }

    #[test]
    fn zero_span_releases_full_from_cliff() {
        // end == start: span is zero, treat as fully released once past cliff.
        let s = sched(1000, 5, 5, 5);
        assert_eq!(s.released(5), 1000);
        assert_eq!(s.locked(5), 0);
    }

    #[test]
    fn locked_is_monotonic_non_increasing() {
        let s = sched(150_000_000_000_000, 0, 2_102_400, 8_409_600);
        let mut prev = u128::MAX;
        for h in (0..=8_500_000).step_by(100_000) {
            let l = s.locked(h);
            assert!(l <= prev, "locked increased at height {h}: {l} > {prev}");
            prev = l;
        }
        assert_eq!(s.locked(0), 150_000_000_000_000);
        assert_eq!(s.locked(2_102_399), 150_000_000_000_000);
        assert_eq!(s.locked(8_409_600), 0);
    }
}
