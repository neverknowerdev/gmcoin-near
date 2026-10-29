//! Spending caps for sponsored (gasless) calls and tick tips.
//!
//! Core pays storage and OutLayer deposits for gasless users and GM tips for
//! tick callers. These counters bound what a buggy or hacked relayer, or a
//! swarm of bots, can make core spend in one day.

#![cfg_attr(not(test), no_std)]

/// A counter that resets when the day changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DailyCounter {
    pub day: u32,
    pub used: u128,
}

impl DailyCounter {
    pub fn used_on(&self, day: u32) -> u128 {
        if self.day == day {
            self.used
        } else {
            0
        }
    }

    /// Adds `amount` if the day's total stays within `limit`.
    pub fn try_add(&mut self, day: u32, amount: u128, limit: u128) -> bool {
        let used = self.used_on(day);
        match used.checked_add(amount) {
            Some(total) if total <= limit => {
                self.day = day;
                self.used = total;
                true
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SponsorLimits {
    /// Sponsored X checks per user per day.
    pub checks_per_user_per_day: u32,
    /// yoctoNEAR core may spend per day on sponsored storage and OutLayer runs.
    pub daily_budget: u128,
    /// Sponsored registrations per day.
    pub registrations_per_day: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Denied {
    UserDailyChecks,
    GlobalBudget,
    GlobalRegistrations,
}

/// Global sponsorship state kept by core.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SponsorState {
    pub spent: DailyCounter,
    pub registrations: DailyCounter,
}

impl SponsorState {
    /// Reserves `cost` for a sponsored registration.
    pub fn register(&mut self, day: u32, cost: u128, lim: &SponsorLimits) -> Result<(), Denied> {
        let mut regs = self.registrations;
        if !regs.try_add(day, 1, lim.registrations_per_day as u128) {
            return Err(Denied::GlobalRegistrations);
        }
        let mut spent = self.spent;
        if !spent.try_add(day, cost, lim.daily_budget) {
            return Err(Denied::GlobalBudget);
        }
        self.registrations = regs;
        self.spent = spent;
        Ok(())
    }

    /// Reserves `cost` for one sponsored X check by a user whose own counter is `user`.
    pub fn check(
        &mut self,
        day: u32,
        cost: u128,
        user: &mut DailyCounter,
        lim: &SponsorLimits,
    ) -> Result<(), Denied> {
        let mut u = *user;
        if !u.try_add(day, 1, lim.checks_per_user_per_day as u128) {
            return Err(Denied::UserDailyChecks);
        }
        let mut spent = self.spent;
        if !spent.try_add(day, cost, lim.daily_budget) {
            return Err(Denied::GlobalBudget);
        }
        *user = u;
        self.spent = spent;
        Ok(())
    }

    /// Gives back an unused reservation (e.g. the OutLayer refund).
    pub fn refund(&mut self, day: u32, amount: u128) {
        if self.spent.day == day {
            self.spent.used = self.spent.used.saturating_sub(amount);
        }
    }
}

/// Tip for a tick caller, bounded by a daily tip budget.
pub fn tip(day: u32, tip_amount: u128, counter: &mut DailyCounter, daily_limit: u128) -> u128 {
    if tip_amount > 0 && counter.try_add(day, tip_amount, daily_limit) {
        tip_amount
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const LIM: SponsorLimits = SponsorLimits {
        checks_per_user_per_day: 3,
        daily_budget: 100,
        registrations_per_day: 2,
    };

    #[test]
    fn counter_resets_on_new_day() {
        let mut c = DailyCounter::default();
        assert!(c.try_add(1, 5, 5));
        assert!(!c.try_add(1, 1, 5));
        assert!(c.try_add(2, 5, 5));
        assert_eq!(c.used_on(1), 0);
    }

    #[test]
    fn user_gets_three_checks_a_day() {
        let mut s = SponsorState::default();
        let mut u = DailyCounter::default();
        for _ in 0..3 {
            assert_eq!(s.check(1, 1, &mut u, &LIM), Ok(()));
        }
        assert_eq!(s.check(1, 1, &mut u, &LIM), Err(Denied::UserDailyChecks));
        assert_eq!(s.check(2, 1, &mut u, &LIM), Ok(()));
    }

    #[test]
    fn global_budget_holds_and_failed_reservations_change_nothing() {
        let mut s = SponsorState::default();
        let mut u = DailyCounter::default();
        assert_eq!(s.check(1, 90, &mut u, &LIM), Ok(()));
        assert_eq!(s.check(1, 20, &mut u, &LIM), Err(Denied::GlobalBudget));
        assert_eq!(u.used, 1, "denied check must not count against the user");
        s.refund(1, 50);
        assert_eq!(s.check(1, 20, &mut u, &LIM), Ok(()));
    }

    #[test]
    fn registrations_are_capped() {
        let mut s = SponsorState::default();
        assert!(s.register(1, 10, &LIM).is_ok());
        assert!(s.register(1, 10, &LIM).is_ok());
        assert_eq!(s.register(1, 10, &LIM), Err(Denied::GlobalRegistrations));
        assert_eq!(s.spent.used, 20);
    }

    #[test]
    fn tips_stop_at_the_daily_limit() {
        let mut c = DailyCounter::default();
        assert_eq!(tip(1, 10, &mut c, 25), 10);
        assert_eq!(tip(1, 10, &mut c, 25), 10);
        assert_eq!(tip(1, 10, &mut c, 25), 0);
        assert_eq!(tip(2, 10, &mut c, 25), 10);
    }

    proptest! {
        #[test]
        fn spend_never_exceeds_budget(costs in proptest::collection::vec(0u128..60, 0..50)) {
            let mut s = SponsorState::default();
            let lim = SponsorLimits { checks_per_user_per_day: u32::MAX, ..LIM };
            let mut u = DailyCounter::default();
            for c in costs {
                let _ = s.check(7, c, &mut u, &lim);
                prop_assert!(s.spent.used_on(7) <= lim.daily_budget);
            }
        }
    }
}
