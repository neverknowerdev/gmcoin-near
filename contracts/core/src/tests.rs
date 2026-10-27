use super::*;
use near_sdk::test_utils::{get_logs, VMContextBuilder};
use near_sdk::{testing_env, PromiseOrValue};

const GM: u128 = 1_000_000_000_000_000_000;
const NEAR: u128 = 1_000_000_000_000_000_000_000_000;
const DAY: u64 = 600 * 1_000_000_000; // 10-minute staging day
const T0: u64 = 1_000 * DAY;

fn acc(s: &str) -> AccountId {
    s.parse().unwrap()
}
fn owner() -> AccountId {
    acc("owner.testnet")
}
fn core_id() -> AccountId {
    acc("core.testnet")
}

pub(crate) fn test_config() -> Config {
    Config {
        emission: EmissionSettings {
            b0: U128(7_000 * GM),
            decay_bps: 150,
            step_cap_bps: 1_000,
            floor_bps: 1_000,
            ceiling_mult: 3,
            baseline_active: 2,
            max_supply: U128(1_000_000 * GM),
            treasury_bps: 300,
            active_min_days: 3,
        },
        unverified_cap: U128(500 * GM),
        tip_amount: U128(GM),
        tip_daily_limit: U128(5 * GM),
        register_deposit: U128(NEAR / 100),
        sponsored_storage_cost: U128(NEAR / 100),
        sponsor_checks_per_user_per_day: 3,
        sponsor_daily_budget: U128(NEAR),
        sponsor_registrations_per_day: 2,
        sponsor_reclaim_after_ns: U64(7 * DAY),
        scan_delay_ns: U64(0),
        scan_batch_size: 2,
        scan_max_in_flight: 2,
        scan_max_attempts: 3,
        settle_chunk: 2,
        scoring: ScoringSettings::default(),
    }
}

fn module() -> ModuleRef {
    ModuleRef {
        source: ModuleSource::WasmUrl {
            url: "https://test.fastfs.io/x/outlayer.testnet/abc.wasm".into(),
            hash: "ab".repeat(32),
            build_target: Some("wasm32-wasip2".into()),
        },
        max_instructions: U64(1_000_000_000),
        max_memory_mb: 128,
        max_execution_seconds: U64(20),
        deposit: U128(NEAR / 10),
    }
}

struct T {
    c: Contract,
    now: u64,
}

impl T {
    fn new() -> Self {
        let mut t = T {
            c: unsafe_placeholder(),
            now: T0,
        };
        t.as_(owner(), 0);
        t.c = Contract::new(
            owner(),
            acc("token.testnet"),
            acc("treasury.testnet"),
            acc("outlayer.testnet"),
            test_config(),
            U64(DAY),
            7,
            Some(U64(T0)),
        );
        t.c.set_modules(Modules {
            x_verify: Some(module()),
            x_scan: Some(module()),
            secrets: None,
        });
        t
    }

    fn as_(&mut self, who: AccountId, deposit: u128) {
        let mut b = VMContextBuilder::new();
        b.current_account_id(core_id())
            .predecessor_account_id(who.clone())
            .signer_account_id(who)
            .attached_deposit(NearToken::from_yoctonear(deposit))
            .account_balance(NearToken::from_near(100))
            .prepaid_gas(Gas::from_tgas(300))
            .random_seed([7; 32])
            .block_timestamp(self.now);
        testing_env!(b.build());
    }

    fn as_core(&mut self) {
        let c = core_id();
        self.as_(c, 0);
    }

    fn at_day(&mut self, day: u32) {
        self.now = T0 + day as u64 * DAY + 1;
    }

    fn register(&mut self, name: &str) -> UserId {
        self.as_(acc(name), NEAR / 100);
        self.c.register().0
    }

    fn link(&mut self, name: &str, x_id: &str) -> UserId {
        let id = self.register(name);
        self.as_(acc(name), 0);
        let code = self.c.start_x_link();
        self.as_(acc(name), NEAR / 10);
        let _ = self.c.verify_x_link(None);
        self.as_core();
        let ok = self.c.on_x_verified(
            U64(id),
            code,
            Ok(Some(VerifyOutput {
                ok: true,
                x_id: Some(x_id.into()),
                handle: Some(format!("h{x_id}")),
                post_id: Some("1".into()),
                error: None,
            })),
        );
        assert!(ok);
        id
    }

    fn scan(&mut self) -> bool {
        self.as_(acc("bell.testnet"), 0);
        matches!(
            self.c.scan_tick(),
            PromiseOrValue::Promise(_) | PromiseOrValue::Value(true)
        )
    }

    fn settle(&mut self) -> bool {
        self.as_(acc("bell.testnet"), 0);
        matches!(
            self.c.settle_day(),
            PromiseOrValue::Promise(_) | PromiseOrValue::Value(true)
        )
    }

    fn scanned(&mut self, day: u32, from: u32, to: u32, scores: Vec<(u64, u64)>) -> bool {
        self.as_core();
        self.c.on_scan(
            day,
            from,
            to,
            Ok(Some(ScanOutput {
                ok: true,
                scores,
                error: None,
            })),
        )
    }

    fn scan_failed(&mut self, day: u32, from: u32, to: u32) -> bool {
        self.as_core();
        self.c.on_scan(day, from, to, Ok(None))
    }

    fn user(&self, name: &str) -> User {
        self.c.get_user(acc(name)).unwrap()
    }
}

// `Contract` has no Default (PanicOnDefault); build one in a test context.
fn unsafe_placeholder() -> Contract {
    let mut b = VMContextBuilder::new();
    b.block_timestamp(T0);
    testing_env!(b.build());
    Contract::new(
        owner(),
        acc("token.testnet"),
        acc("treasury.testnet"),
        acc("outlayer.testnet"),
        test_config(),
        U64(DAY),
        7,
        Some(U64(T0)),
    )
}

#[test]
fn init_opens_epoch_zero_with_b0() {
    let t = T::new();
    let e = t.c.get_epoch(0).unwrap();
    assert_eq!(e.budget.0, 7_000 * GM);
    assert_eq!(e.daily_pool.0, 1_000 * GM);
    assert_eq!(t.c.status().today, 0);
}

#[test]
fn register_with_deposit_refunds_excess() {
    let mut t = T::new();
    t.as_(acc("alice.testnet"), NEAR);
    let id = t.c.register();
    assert_eq!(id.0, 1);
    let u = t.user("alice.testnet");
    assert!(!u.sponsored);
    assert_eq!(u.storage_deposit.0, NEAR / 100);
}

#[test]
#[should_panic(expected = "already registered")]
fn register_twice_fails() {
    let mut t = T::new();
    t.register("alice.testnet");
    t.register("alice.testnet");
}

#[test]
#[should_panic(expected = "attach register_deposit or nothing")]
fn register_with_too_little_fails() {
    let mut t = T::new();
    t.as_(acc("alice.testnet"), 1);
    t.c.register();
}

#[test]
fn sponsored_registration_is_capped_per_day() {
    let mut t = T::new();
    t.as_(acc("a.testnet"), 0);
    t.c.register();
    t.as_(acc("b.testnet"), 0);
    t.c.register();
    assert!(t.user("a.testnet").sponsored);
    t.as_(acc("c.testnet"), 0);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| t.c.register()));
    assert!(
        r.is_err(),
        "third sponsored registration of the day must fail"
    );
}

#[test]
fn unregister_refunds_and_removes() {
    let mut t = T::new();
    t.register("alice.testnet");
    t.as_(acc("alice.testnet"), 0);
    t.c.unregister();
    assert!(t.c.get_user(acc("alice.testnet")).is_none());
    assert_eq!(t.c.status().users, 0);
}

#[test]
fn sponsored_users_without_x_are_reclaimed_after_a_week() {
    let mut t = T::new();
    t.as_(acc("a.testnet"), 0);
    t.c.register();
    t.as_(acc("anyone.testnet"), 0);
    let early = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        t.c.reclaim_sponsored(acc("a.testnet"))
    }));
    assert!(early.is_err());
    t.at_day(7);
    t.as_(acc("anyone.testnet"), 0);
    t.c.reclaim_sponsored(acc("a.testnet"));
    assert!(t.c.get_user(acc("a.testnet")).is_none());
}

#[test]
fn link_code_belongs_to_the_account() {
    let mut t = T::new();
    t.register("alice.testnet");
    t.as_(acc("alice.testnet"), 0);
    let code = t.c.start_x_link();
    assert!(gm_scoring::link_code::validate(&code, "alice.testnet"));
    assert_eq!(
        t.c.get_pending_link(acc("alice.testnet")).unwrap().code,
        code
    );
}

#[test]
fn x_link_happy_path() {
    let mut t = T::new();
    t.link("alice.testnet", "111");
    let u = t.user("alice.testnet");
    assert_eq!(u.x_id.as_deref(), Some("111"));
    assert_eq!(u.x_handle.as_deref(), Some("h111"));
    assert!(t.c.get_pending_link(acc("alice.testnet")).is_none());
    assert_eq!(
        t.c.get_user_by_x("111".into()).unwrap().near_account,
        acc("alice.testnet")
    );
    assert!(get_logs().iter().any(|l| l.contains("x_linked")));
}

#[test]
fn x_id_is_unique_across_users() {
    let mut t = T::new();
    t.link("alice.testnet", "111");
    let id = t.register("bob.testnet");
    t.as_(acc("bob.testnet"), 0);
    let code = t.c.start_x_link();
    t.as_(acc("bob.testnet"), NEAR / 10);
    let _ = t.c.verify_x_link(None);
    t.as_core();
    let ok = t.c.on_x_verified(
        U64(id),
        code,
        Ok(Some(VerifyOutput {
            ok: true,
            x_id: Some("111".into()),
            ..Default::default()
        })),
    );
    assert!(!ok);
    let p = t.c.get_pending_link(acc("bob.testnet")).unwrap();
    assert_eq!(p.last_error.as_deref(), Some("X account already linked"));
    assert!(!p.in_flight);
}

#[test]
fn failed_or_stale_verification_does_not_link() {
    let mut t = T::new();
    let id = t.register("alice.testnet");
    t.as_(acc("alice.testnet"), 0);
    let code = t.c.start_x_link();
    t.as_(acc("alice.testnet"), NEAR / 10);
    let _ =
        t.c.verify_x_link(Some("https://x.com/alice/status/123".into()));
    t.as_core();
    // A callback for an old code is ignored.
    assert!(!t.c.on_x_verified(U64(id), "GM00OLD".into(), Ok(None)));
    assert!(!t.c.on_x_verified(U64(id), code, Ok(None)));
    let p = t.c.get_pending_link(acc("alice.testnet")).unwrap();
    assert_eq!(p.last_error.as_deref(), Some("OutLayer run failed"));
    assert!(t.user("alice.testnet").x_id.is_none());
}

#[test]
#[should_panic(expected = "verification in progress")]
fn one_verification_at_a_time() {
    let mut t = T::new();
    t.register("alice.testnet");
    t.as_(acc("alice.testnet"), 0);
    t.c.start_x_link();
    t.as_(acc("alice.testnet"), NEAR / 10);
    let _ = t.c.verify_x_link(None);
    let _ = t.c.verify_x_link(None);
}

#[test]
fn sponsored_checks_are_capped_per_user() {
    let mut t = T::new();
    t.as_(acc("a.testnet"), 0);
    let id = t.c.register().0;
    for _ in 0..3 {
        t.as_(acc("a.testnet"), 0);
        let code = t.c.start_x_link();
        t.as_(acc("a.testnet"), 0);
        let _ = t.c.verify_x_link(None);
        t.as_core();
        t.c.on_x_verified(U64(id), code, Ok(None));
    }
    t.as_(acc("a.testnet"), 0);
    t.c.start_x_link();
    t.as_(acc("a.testnet"), 0);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| t.c.verify_x_link(None)));
    assert!(r.is_err(), "fourth sponsored check of the day must fail");
}

#[test]
#[should_panic(expected = "bad post link")]
fn bad_post_link_is_refused() {
    let mut t = T::new();
    t.register("alice.testnet");
    t.as_(acc("alice.testnet"), 0);
    t.c.start_x_link();
    t.as_(acc("alice.testnet"), NEAR / 10);
    let _ = t.c.verify_x_link(Some("https://x.com/alice".into()));
}

#[test]
fn ticks_do_nothing_before_the_day_ends() {
    let mut t = T::new();
    t.link("alice.testnet", "1");
    assert!(!t.scan());
    assert!(!t.settle());
    t.as_(acc("bell.testnet"), 0);
    assert!(!t.c.close_epoch());
}

#[test]
fn empty_day_is_scanned_and_skipped_without_minting() {
    let mut t = T::new();
    t.at_day(1);
    assert!(t.scan());
    assert_eq!(t.c.get_day(0).unwrap().status, DayStatus::Scanned);
    assert!(!t.scan(), "a repeated tick does nothing");
    assert!(t.settle());
    assert_eq!(t.c.get_day(0).unwrap().status, DayStatus::Skipped);
    assert_eq!(t.c.status().settle_day, 1);
}

fn three_users() -> T {
    let mut t = T::new();
    t.link("a.testnet", "1"); // uid 1
    t.link("b.testnet", "2"); // uid 2
    t.link("c.testnet", "3"); // uid 3
    t
}

#[test]
fn full_day_scans_in_batches_and_settles_by_points() {
    let mut t = three_users();
    t.at_day(1);
    assert!(t.scan()); // batch [0,2)
    assert!(t.scan()); // batch [2,3)
    assert!(!t.scan(), "in-flight limit / nothing left");
    assert!(t.scanned(0, 0, 2, vec![(1, 30), (2, 10)]));
    assert_eq!(t.c.get_day(0).unwrap().status, DayStatus::Scanning);
    assert!(t.scanned(0, 2, 3, vec![(3, 60)]));
    let day = t.c.get_day(0).unwrap();
    assert_eq!(day.status, DayStatus::Scanned);
    assert_eq!(day.total_points, 100);

    // Tips: 2 scan ticks = 2 GM, they come out of the pool.
    assert!(t.settle()); // chunk 1: treasury + users 1,2
    assert!(t.settle()); // chunk 2: user 3
    let day = t.c.get_day(0).unwrap();
    assert_eq!(day.status, DayStatus::Settled);
    let pool = 1_000 * GM;
    assert_eq!(day.pool.0, pool);
    assert_eq!(day.treasury.0, 30 * GM);
    assert_eq!(day.tips.0, 2 * GM);
    let users_pool = pool - 30 * GM - 2 * GM;
    assert_eq!(day.users_pool.0, users_pool);
    assert_eq!(t.user("a.testnet").accrued.0, users_pool * 30 / 100);
    // c earns 58% of the pool but is capped by the $1 rule for unverified users.
    assert_eq!(
        t.user("c.testnet").accrued.0,
        (users_pool * 60 / 100).min(500 * GM)
    );
    assert!(day.minted.0 <= pool);
    assert!(!t.settle(), "nothing left to settle");
}

#[test]
fn scan_ignores_users_outside_the_batch_and_duplicates() {
    let mut t = three_users();
    t.at_day(1);
    t.scan();
    t.scan();
    t.scanned(0, 0, 2, vec![(1, 5), (1, 5), (3, 1_000), (99, 7), (2, 0)]);
    t.scanned(0, 2, 3, vec![]);
    let day = t.c.get_day(0).unwrap();
    assert_eq!(day.total_points, 5);
    assert_eq!(day.score_count, 1);
}

#[test]
fn failed_batches_retry_then_skip_the_day() {
    let mut t = three_users();
    t.at_day(1);
    t.scan();
    t.scan();
    t.scanned(0, 2, 3, vec![(3, 9)]);
    t.scan_failed(0, 0, 2);
    assert_eq!(t.c.get_day(0).unwrap().retry.len(), 1);
    assert!(t.scan()); // retry
    t.scan_failed(0, 0, 2);
    assert!(t.scan());
    t.scan_failed(0, 0, 2);
    let day = t.c.get_day(0).unwrap();
    assert_eq!(day.status, DayStatus::Failed);
    assert_eq!(t.c.status().scan_day, 1, "the pipeline moves on");
    assert!(t.settle());
    assert_eq!(t.c.get_day(0).unwrap().status, DayStatus::Skipped);
    assert_eq!(
        t.user("c.testnet").accrued.0,
        0,
        "failed day pool is never minted"
    );
    // Late callback after the day failed is ignored.
    assert!(!t.scanned(0, 0, 2, vec![(1, 5)]));
}

#[test]
fn unverified_cap_limits_minted_rewards() {
    let mut t = T::new();
    t.link("a.testnet", "1");
    t.at_day(1);
    t.scan();
    t.scanned(0, 0, 1, vec![(1, 10)]);
    t.settle();
    assert_eq!(t.user("a.testnet").accrued.0, 500 * GM);
    assert_eq!(t.user("a.testnet").minted_unverified.0, 500 * GM);

    t.as_(owner(), 0);
    t.c.set_verified(acc("a.testnet"), true);
    t.at_day(2);
    t.scan();
    t.scanned(1, 0, 1, vec![(1, 10)]);
    t.settle();
    assert!(t.user("a.testnet").accrued.0 > 1_000 * GM);
}

#[test]
fn failed_mint_is_retried_with_the_same_batch_id() {
    let mut t = T::new();
    t.link("a.testnet", "1");
    t.at_day(1);
    t.scan();
    t.scanned(0, 0, 1, vec![(1, 10)]);
    t.settle();
    // Batch ids: 1 = scan tip, 2 = settle tip, 3 = settle chunk.
    t.as_core();
    assert!(!t.c.on_minted(U64(3), Err(near_sdk::PromiseError::Failed)));
    assert_eq!(t.c.status().failed_mints, 1);
    assert!(t.settle(), "retry counts as work");
    assert_eq!(t.c.status().failed_mints, 0);
    t.as_core();
    assert!(t.c.on_minted(U64(3), Ok(())));
    assert_eq!(t.user("a.testnet").delivered, t.user("a.testnet").accrued);
}

#[test]
fn tips_are_capped_per_day() {
    let mut t = T::new();
    // 6 empty days -> 6 scan ticks + 6 settle ticks on the same real day,
    // but the daily tip limit is 5 GM.
    t.at_day(7);
    for _ in 0..7 {
        assert!(t.scan());
        assert!(t.settle());
    }
    assert_eq!(t.c.status().total_minted.0, 5 * GM);
}

#[test]
fn epoch_closes_after_all_days_settle_and_budget_moves_within_bounds() {
    let mut t = three_users();
    t.at_day(7);
    t.as_(acc("bell.testnet"), 0);
    assert!(!t.c.close_epoch(), "days not settled yet");
    for d in 0..7 {
        t.scan();
        t.scan();
        t.scanned(d, 0, 2, vec![(1, 3), (2, 3)]);
        t.scanned(d, 2, 3, vec![(3, 3)]);
        while t.settle() {}
    }
    assert_eq!(t.c.status().settle_day, 7);
    t.as_(acc("bell.testnet"), 0);
    assert!(t.c.close_epoch());
    assert!(!t.c.close_epoch(), "repeated close does nothing");
    let closed = t.c.get_epoch(0).unwrap();
    assert!(closed.closed);
    assert_eq!(closed.active_users, 3);
    let next = t.c.get_epoch(1).unwrap();
    // baseline 2, active 3 -> sqrt(1.5) > 1, capped at +10%.
    assert_eq!(next.budget.0, 7_700 * GM);
    assert_eq!(t.c.leaderboard(Some(0)).len(), 3);
    assert_eq!(t.c.epoch_history(None, None).len(), 2);
}

#[test]
fn settlement_waits_for_the_epoch_budget() {
    let mut t = T::new();
    t.at_day(8);
    for _ in 0..8 {
        t.scan();
    }
    assert_eq!(t.c.status().scan_day, 8);
    for _ in 0..7 {
        assert!(t.settle());
    }
    assert!(!t.settle(), "day 7 needs epoch 1 to exist");
    t.as_(acc("bell.testnet"), 0);
    assert!(t.c.close_epoch());
    assert!(t.settle());
}

#[test]
#[should_panic(expected = "paused")]
fn pause_stops_ticks() {
    let mut t = T::new();
    t.as_(owner(), 0);
    t.c.set_paused(true);
    t.at_day(1);
    t.scan();
}

#[test]
#[should_panic(expected = "only owner")]
fn only_owner_sets_config() {
    let mut t = T::new();
    t.as_(acc("x.testnet"), 0);
    t.c.set_config(test_config());
}

#[test]
fn export_views_page_through_users() {
    let mut t = T::new();
    for i in 0..5 {
        t.register(&format!("u{i}.testnet"));
    }
    assert_eq!(t.c.export_users(None, Some(2)).len(), 2);
    assert_eq!(t.c.export_users(Some(U64(4)), Some(10)).len(), 2);
}

#[test]
fn estimate_today_uses_the_unsettled_day() {
    let mut t = three_users();
    t.at_day(1);
    t.scan();
    t.scan();
    t.scanned(0, 0, 2, vec![(1, 25), (2, 25)]);
    let e = t.c.estimate_today(acc("a.testnet"));
    assert_eq!(e.day, Some(0));
    assert_eq!(e.estimate.0, (1_000 * GM - 30 * GM) / 2);
}
