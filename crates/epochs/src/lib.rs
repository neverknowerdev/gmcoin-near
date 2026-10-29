//! Pure GM emission math.
//!
//! GM has a fixed weekly budget. Each day of an epoch gets `budget / days`,
//! split by points; the treasury share comes out of the pool, not on top.
//! At every epoch close the next budget follows a decaying schedule scaled by
//! real activity, bounded by a step cap, a floor and a ceiling.
//!
//! Nothing here touches a chain, so every rule is unit- and property-tested.

/// Basis points denominator.
pub const BPS: u128 = 10_000;
/// Fixed-point scale used for the decay factor.
const WAD: u128 = 1_000_000_000_000_000_000;
/// Fixed-point scale used for the activity multiplier (sqrt keeps it in range).
const SCALE: u128 = 1_000_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmissionConfig {
    /// Starting weekly budget, in token base units.
    pub b0: u128,
    /// Weekly schedule decay, e.g. 150 = 1.5%.
    pub decay_bps: u32,
    /// Max change of the budget from one epoch to the next, e.g. 1000 = 10%.
    pub step_cap_bps: u32,
    /// Tail emission floor as a share of `b0`, e.g. 1000 = 10%.
    pub floor_bps: u32,
    /// Ceiling as a multiple of the average of the last 4 budgets.
    pub ceiling_mult: u32,
    /// Active users at which the activity multiplier is exactly 1.
    pub baseline_active: u64,
}

impl EmissionConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.b0 == 0 {
            return Err("b0 must be positive");
        }
        if self.decay_bps as u128 >= BPS {
            return Err("decay_bps must be below 10000");
        }
        if self.step_cap_bps as u128 > BPS {
            return Err("step_cap_bps must be at most 10000");
        }
        if self.floor_bps as u128 > BPS {
            return Err("floor_bps must be at most 10000");
        }
        if self.ceiling_mult == 0 {
            return Err("ceiling_mult must be positive");
        }
        if self.baseline_active == 0 {
            return Err("baseline_active must be positive");
        }
        Ok(())
    }

    pub fn floor(&self) -> u128 {
        self.b0 * self.floor_bps as u128 / BPS
    }
}

/// `b0 * (1 - decay)^n`, rounded down.
pub fn schedule(cfg: &EmissionConfig, n: u32) -> u128 {
    let factor = (BPS - cfg.decay_bps as u128) * (WAD / BPS);
    mul_wad(cfg.b0, pow_wad(factor, n))
}

/// Activity multiplier `clamp(sqrt(active / baseline), 0.5, 2)` in `SCALE` units.
/// A week with no active users keeps the schedule value (multiplier 1).
pub fn activity_multiplier(active: u64, baseline: u64) -> u128 {
    if active == 0 || baseline == 0 {
        return SCALE;
    }
    // sqrt(active / baseline) * SCALE == sqrt(active * SCALE^2 / baseline)
    let ratio = (active as u128) * SCALE * SCALE / (baseline as u128);
    isqrt(ratio).clamp(SCALE / 2, SCALE * 2)
}

/// Inputs for the budget of epoch `next_index`.
#[derive(Clone, Debug)]
pub struct BudgetInput<'a> {
    pub next_index: u32,
    pub prev_budget: u128,
    /// Linked X accounts that scored on at least 3 days of the closing epoch.
    pub active_users: u64,
    /// Budgets of the most recent epochs, newest last (the closing one included).
    pub recent_budgets: &'a [u128],
}

/// The next weekly budget, after the schedule, activity, step cap, floor and ceiling.
pub fn next_budget(cfg: &EmissionConfig, input: &BudgetInput) -> u128 {
    let sched = schedule(cfg, input.next_index);
    let target = mul_div(
        sched,
        activity_multiplier(input.active_users, cfg.baseline_active),
        SCALE,
    );

    let prev = input.prev_budget;
    let step = prev * cfg.step_cap_bps as u128 / BPS;
    let mut budget = target.clamp(prev.saturating_sub(step), prev.saturating_add(step));

    let recent: Vec<u128> = input.recent_budgets.iter().rev().take(4).copied().collect();
    if !recent.is_empty() {
        let avg = recent.iter().sum::<u128>() / recent.len() as u128;
        budget = budget.min(avg * cfg.ceiling_mult as u128);
    }
    // The floor wins over the ceiling, so emission never fades to nothing.
    budget.max(cfg.floor())
}

/// Pool for one day: `budget / days`, plus dust carried from earlier days.
pub fn daily_pool(budget: u128, days_per_epoch: u32, carried_dust: u128) -> u128 {
    budget / days_per_epoch.max(1) as u128 + carried_dust
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DaySplit {
    pub treasury: u128,
    /// Amount shared between users by points.
    pub users_pool: u128,
}

/// Splits a day's pool into the treasury share and the users' part.
/// `tips` (GM already paid to tick callers that day) also come out of the pool.
pub fn split_pool(pool: u128, treasury_bps: u32, tips: u128) -> DaySplit {
    let treasury = pool * treasury_bps as u128 / BPS;
    let users_pool = pool.saturating_sub(treasury).saturating_sub(tips);
    DaySplit {
        treasury,
        users_pool,
    }
}

/// A user's share: `users_pool * points / total_points`, rounded down.
pub fn payout(users_pool: u128, points: u64, total_points: u64) -> u128 {
    if total_points == 0 {
        return 0;
    }
    mul_div(users_pool, points as u128, total_points as u128)
}

/// Caps a payout so that an unverified user's lifetime minted rewards stay
/// under `cap`. Returns the amount that may be minted.
pub fn apply_unverified_cap(amount: u128, minted_unverified: u128, cap: u128) -> u128 {
    amount.min(cap.saturating_sub(minted_unverified))
}

/// Caps a mint so total supply never passes `max_supply`.
pub fn apply_max_supply(amount: u128, total_supply: u128, max_supply: u128) -> u128 {
    amount.min(max_supply.saturating_sub(total_supply))
}

/// Calendar helpers. Days and epochs are fixed-length windows from `genesis`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calendar {
    pub genesis_ns: u64,
    pub day_ns: u64,
    pub days_per_epoch: u32,
}

impl Calendar {
    pub fn day_index(&self, now_ns: u64) -> u32 {
        (now_ns.saturating_sub(self.genesis_ns) / self.day_ns.max(1)) as u32
    }

    pub fn epoch_of_day(&self, day: u32) -> u32 {
        day / self.days_per_epoch.max(1)
    }

    pub fn day_start_ns(&self, day: u32) -> u64 {
        self.genesis_ns + day as u64 * self.day_ns
    }

    pub fn first_day_of_epoch(&self, epoch: u32) -> u32 {
        epoch * self.days_per_epoch
    }
}

/// `a * b / d` rounded down, exact while `b <= d` or `(d - 1) * b` fits in u128.
pub fn mul_div(a: u128, b: u128, d: u128) -> u128 {
    match a.checked_mul(b) {
        Some(p) => p / d,
        None => (a / d) * b + (a % d) * b / d,
    }
}

fn mul_wad(a: u128, b: u128) -> u128 {
    // a * b / WAD without overflow for a up to ~3.4e20 * WAD.
    let (ah, al) = (a / WAD, a % WAD);
    ah * b + al * b / WAD
}

fn pow_wad(mut base: u128, mut exp: u32) -> u128 {
    let mut acc = WAD;
    while exp > 0 {
        if exp & 1 == 1 {
            acc = acc * (base / 1_000_000_000) / 1_000_000_000;
        }
        base = base * (base / 1_000_000_000) / 1_000_000_000;
        exp >>= 1;
    }
    acc
}

fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = 1u128 << ((128 - n.leading_zeros()).div_ceil(2));
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const GM: u128 = 1_000_000_000_000_000_000;

    fn cfg() -> EmissionConfig {
        EmissionConfig {
            b0: 1_000_000 * GM,
            decay_bps: 150,
            step_cap_bps: 1_000,
            floor_bps: 1_000,
            ceiling_mult: 3,
            baseline_active: 1_000,
        }
    }

    #[test]
    fn schedule_decays_one_and_a_half_percent_a_week() {
        let c = cfg();
        assert_eq!(schedule(&c, 0), c.b0);
        assert_eq!(schedule(&c, 1), 985_000 * GM);
        let s52 = schedule(&c, 52);
        // 0.985^52 = 0.45568...
        assert!(s52 > 455_600 * GM && s52 < 455_800 * GM, "{s52}");
    }

    #[test]
    fn activity_multiplier_is_sqrt_and_clamped() {
        assert_eq!(activity_multiplier(1_000, 1_000), SCALE);
        assert_eq!(activity_multiplier(4_000, 1_000), 2 * SCALE);
        assert_eq!(activity_multiplier(250, 1_000), SCALE / 2);
        assert_eq!(activity_multiplier(1_000_000, 1_000), 2 * SCALE);
        assert_eq!(activity_multiplier(1, 1_000), SCALE / 2);
        assert_eq!(activity_multiplier(0, 1_000), SCALE);
        let m = activity_multiplier(2_000, 1_000);
        assert!(m > 1_414_213_000_000 && m < 1_414_214_000_000);
    }

    #[test]
    fn budget_moves_at_most_ten_percent() {
        let c = cfg();
        let up = next_budget(
            &c,
            &BudgetInput {
                next_index: 1,
                prev_budget: c.b0,
                active_users: 10_000,
                recent_budgets: &[c.b0],
            },
        );
        assert_eq!(up, 1_100_000 * GM);
        let down = next_budget(
            &c,
            &BudgetInput {
                next_index: 1,
                prev_budget: c.b0,
                active_users: 10,
                recent_budgets: &[c.b0],
            },
        );
        assert_eq!(down, 900_000 * GM);
    }

    #[test]
    fn no_active_users_keeps_schedule() {
        let c = cfg();
        let b = next_budget(
            &c,
            &BudgetInput {
                next_index: 1,
                prev_budget: c.b0,
                active_users: 0,
                recent_budgets: &[c.b0],
            },
        );
        assert_eq!(b, schedule(&c, 1));
    }

    #[test]
    fn floor_holds_after_long_decay() {
        let c = cfg();
        let b = next_budget(
            &c,
            &BudgetInput {
                next_index: 500,
                prev_budget: c.floor(),
                active_users: 1,
                recent_budgets: &[c.floor()],
            },
        );
        assert_eq!(b, c.floor());
    }

    #[test]
    fn ceiling_caps_against_recent_average() {
        let mut c = cfg();
        c.step_cap_bps = 10_000;
        c.ceiling_mult = 1;
        let b = next_budget(
            &c,
            &BudgetInput {
                next_index: 0,
                prev_budget: c.b0,
                active_users: 4_000,
                recent_budgets: &[c.b0 * 100, c.b0 / 2, c.b0 / 2, c.b0 / 2, c.b0 / 2],
            },
        );
        // Only the newest four count, so the 100x outlier is ignored.
        assert_eq!(b, c.b0 / 2);
    }

    #[test]
    fn split_takes_treasury_and_tips_from_the_pool() {
        let s = split_pool(1_000, 300, 10);
        assert_eq!(
            s,
            DaySplit {
                treasury: 30,
                users_pool: 960
            }
        );
        assert_eq!(split_pool(10, 300, 100).users_pool, 0);
    }

    #[test]
    fn caps() {
        assert_eq!(apply_unverified_cap(100, 950, 1_000), 50);
        assert_eq!(apply_unverified_cap(100, 2_000, 1_000), 0);
        assert_eq!(apply_max_supply(100, 990, 1_000), 10);
        assert_eq!(payout(100, 1, 0), 0);
    }

    #[test]
    fn calendar() {
        let cal = Calendar {
            genesis_ns: 1_000,
            day_ns: 10,
            days_per_epoch: 7,
        };
        assert_eq!(cal.day_index(999), 0);
        assert_eq!(cal.day_index(1_075), 7);
        assert_eq!(cal.epoch_of_day(7), 1);
        assert_eq!(cal.day_start_ns(7), 1_070);
        assert_eq!(cal.first_day_of_epoch(2), 14);
    }

    #[test]
    fn validate_rejects_bad_configs() {
        assert!(cfg().validate().is_ok());
        let mut c = cfg();
        c.decay_bps = 10_000;
        assert!(c.validate().is_err());
        let mut c = cfg();
        c.baseline_active = 0;
        assert!(c.validate().is_err());
    }

    proptest! {
        #[test]
        fn budget_respects_step_floor_and_ceiling(
            b0 in 1u128..1_000_000_000 * GM,
            prev_frac in 1u128..300,
            n in 0u32..2_000,
            active in 0u64..10_000_000,
            baseline in 1u64..100_000,
        ) {
            let c = EmissionConfig { b0, decay_bps: 150, step_cap_bps: 1_000, floor_bps: 1_000, ceiling_mult: 3, baseline_active: baseline };
            let prev = (b0 * prev_frac / 100).max(c.floor());
            let recent = [prev];
            let b = next_budget(&c, &BudgetInput { next_index: n, prev_budget: prev, active_users: active, recent_budgets: &recent });
            prop_assert!(b >= c.floor());
            let ceil = prev * 3;
            prop_assert!(b <= ceil.max(c.floor()));
            let step = prev / 10;
            // Either within 10% of prev, or pulled to the floor.
            prop_assert!(b == c.floor() || (b + step + 1 >= prev && b <= prev + step + 1));
        }

        #[test]
        fn payouts_never_exceed_pool(pool in 0u128..u64::MAX as u128 * GM, points in proptest::collection::vec(0u64..10_000, 0..200)) {
            let total: u64 = points.iter().sum();
            let paid: u128 = points.iter().map(|p| payout(pool, *p, total)).sum();
            prop_assert!(paid <= pool);
            // Dust is below one unit per payee.
            if total > 0 { prop_assert!(pool - paid <= points.len() as u128); }
        }

        #[test]
        fn schedule_is_monotonic(b0 in 1u128..1_000_000_000 * GM, n in 0u32..3_000) {
            let c = EmissionConfig { b0, decay_bps: 150, step_cap_bps: 1_000, floor_bps: 1_000, ceiling_mult: 3, baseline_active: 1 };
            prop_assert!(schedule(&c, n + 1) <= schedule(&c, n));
        }
    }
}
