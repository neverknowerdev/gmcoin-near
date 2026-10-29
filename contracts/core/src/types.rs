use near_sdk::json_types::{U128, U64};
use near_sdk::{near, AccountId};

pub type UserId = u64;
pub type DayIndex = u32;
pub type EpochIndex = u32;

/// Day counter that resets when the day changes (stored form of `caps::DailyCounter`).
#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counter {
    pub day: u32,
    pub used: U128,
}

impl From<Counter> for caps::DailyCounter {
    fn from(c: Counter) -> Self {
        caps::DailyCounter {
            day: c.day,
            used: c.used.0,
        }
    }
}

impl From<caps::DailyCounter> for Counter {
    fn from(c: caps::DailyCounter) -> Self {
        Counter {
            day: c.day,
            used: U128(c.used),
        }
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub struct User {
    pub id: UserId,
    pub near_account: AccountId,
    pub x_id: Option<String>,
    pub x_handle: Option<String>,
    /// GM assigned by settlements (lifetime).
    pub accrued: U128,
    /// GM confirmed minted by the token (lifetime).
    pub delivered: U128,
    /// Rewards minted while not verified as human; capped by `unverified_cap`.
    pub minted_unverified: U128,
    pub verified_human: bool,
    /// Core paid this user's storage (gasless onboarding).
    pub sponsored: bool,
    /// NEAR the user attached for storage at registration (refunded on unregister).
    pub storage_deposit: U128,
    pub created_at: U64,
    pub linked_at: Option<U64>,
    /// Sponsored X checks used today.
    pub checks: Counter,
    /// Epoch whose active-days count is in `active_days`.
    pub active_epoch: EpochIndex,
    pub active_days: u8,
    /// Points in `points_epoch`, for the leaderboard.
    pub points_epoch: EpochIndex,
    pub epoch_points: u64,
    pub last_scored_day: Option<DayIndex>,
    pub last_day_points: u64,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingLink {
    pub code: String,
    pub created_at: U64,
    pub in_flight: bool,
    pub last_error: Option<String>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayStatus {
    /// Waiting for (more) scan batches.
    Scanning,
    /// All batches recorded; ready to settle.
    Scanned,
    /// A batch failed too often; the day is skipped and its pool not minted.
    Failed,
    /// Minting chunks.
    Settling,
    Settled,
    /// Nothing to mint (failed scan or zero points). Pool not rolled over.
    Skipped,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanBatch {
    /// Range of positions in the linked-users list, `[from, to)`.
    pub from: u32,
    pub to: u32,
    pub attempts: u8,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub struct Day {
    pub index: DayIndex,
    pub status: DayStatus,
    pub scan_cursor: u32,
    pub scan_end: u32,
    pub in_flight: Vec<ScanBatch>,
    pub retry: Vec<ScanBatch>,
    pub score_count: u32,
    pub total_points: u64,
    pub pool: U128,
    pub treasury: U128,
    pub users_pool: U128,
    pub tips: U128,
    pub settle_cursor: u32,
    /// Sum of raw payouts before caps (the rest of `users_pool` is dust).
    pub assigned: U128,
    /// Actually sent to the token after the $1 and supply caps.
    pub minted: U128,
}

impl Day {
    pub fn new(index: DayIndex, linked: u32) -> Self {
        Self {
            index,
            status: DayStatus::Scanning,
            scan_cursor: 0,
            scan_end: linked,
            in_flight: vec![],
            retry: vec![],
            score_count: 0,
            total_points: 0,
            pool: U128(0),
            treasury: U128(0),
            users_pool: U128(0),
            tips: U128(0),
            settle_cursor: 0,
            assigned: U128(0),
            minted: U128(0),
        }
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub struct Epoch {
    pub index: EpochIndex,
    pub start_day: DayIndex,
    pub budget: U128,
    pub daily_pool: U128,
    /// Filled when the epoch closes.
    pub active_users: u64,
    pub total_points: u64,
    pub minted: U128,
    pub closed: bool,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub struct MintBatch {
    pub id: U64,
    pub day: DayIndex,
    /// (user id or `None` for treasury / tip receivers, account, amount)
    pub entries: Vec<(Option<UserId>, AccountId, U128)>,
}

/// Where an OutLayer module's code comes from (OutLayer's `ExecutionSource`).
#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub enum ModuleSource {
    GitHub {
        repo: String,
        commit: String,
        build_target: Option<String>,
    },
    WasmUrl {
        url: String,
        hash: String,
        build_target: Option<String>,
    },
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub struct ModuleRef {
    pub source: ModuleSource,
    pub max_instructions: U64,
    pub max_memory_mb: u32,
    pub max_execution_seconds: U64,
    /// NEAR attached to `request_execution`; the unused part is refunded to the payer.
    pub deposit: U128,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub struct SecretsRef {
    pub profile: String,
    pub account_id: AccountId,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Modules {
    pub x_verify: Option<ModuleRef>,
    pub x_scan: Option<ModuleRef>,
    pub secrets: Option<SecretsRef>,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub struct EmissionSettings {
    pub b0: U128,
    pub decay_bps: u32,
    pub step_cap_bps: u32,
    pub floor_bps: u32,
    pub ceiling_mult: u32,
    pub baseline_active: u64,
    /// Mirror of the token's MAX_SUPPLY; core clamps mints to it.
    pub max_supply: U128,
    pub treasury_bps: u32,
    /// Days a user must score in an epoch to count as active.
    pub active_min_days: u8,
}

impl EmissionSettings {
    pub fn to_config(&self) -> epochs::EmissionConfig {
        epochs::EmissionConfig {
            b0: self.b0.0,
            decay_bps: self.decay_bps,
            step_cap_bps: self.step_cap_bps,
            floor_bps: self.floor_bps,
            ceiling_mult: self.ceiling_mult,
            baseline_active: self.baseline_active,
        }
    }
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub emission: EmissionSettings,
    /// Lifetime GM an unverified user may be minted as rewards ($1 at a fixed price).
    pub unverified_cap: U128,
    /// GM tip per tick that did work.
    pub tip_amount: U128,
    /// Max GM tips per day.
    pub tip_daily_limit: U128,
    /// NEAR a user attaches at `register` to pay their own storage.
    pub register_deposit: U128,
    /// What core budgets for a sponsored registration.
    pub sponsored_storage_cost: U128,
    pub sponsor_checks_per_user_per_day: u32,
    pub sponsor_daily_budget: U128,
    pub sponsor_registrations_per_day: u32,
    /// Sponsored users without X after this long can be removed.
    pub sponsor_reclaim_after_ns: U64,
    /// Wait after a day ends before scanning it, so likes accumulate.
    pub scan_delay_ns: U64,
    pub scan_batch_size: u32,
    pub scan_max_in_flight: u32,
    pub scan_max_attempts: u8,
    pub settle_chunk: u32,
    pub scoring: ScoringSettings,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub struct ScoringSettings {
    pub points_simple: u32,
    pub points_hashtag: u32,
    pub points_cashtag: u32,
    pub points_per_like: u32,
    pub max_hashtag_posts: u32,
    pub max_cashtag_posts: u32,
    pub max_simple_posts: u32,
    pub max_likes_per_post: u32,
    pub spam_post_limit: u32,
}

impl Default for ScoringSettings {
    fn default() -> Self {
        let d = gm_scoring::ScoringConfig::default();
        Self {
            points_simple: d.points_simple,
            points_hashtag: d.points_hashtag,
            points_cashtag: d.points_cashtag,
            points_per_like: d.points_per_like,
            max_hashtag_posts: d.max_hashtag_posts,
            max_cashtag_posts: d.max_cashtag_posts,
            max_simple_posts: d.max_simple_posts,
            max_likes_per_post: d.max_likes_per_post,
            spam_post_limit: d.spam_post_limit,
        }
    }
}

/// Output of the x-verify module.
#[near(serializers = [json])]
#[derive(Clone, Debug, Default)]
pub struct VerifyOutput {
    pub ok: bool,
    pub x_id: Option<String>,
    pub handle: Option<String>,
    pub post_id: Option<String>,
    pub error: Option<String>,
}

/// Output of the x-scan module: `[user id, points]` pairs.
#[near(serializers = [json])]
#[derive(Clone, Debug, Default)]
pub struct ScanOutput {
    pub ok: bool,
    #[serde(default)]
    pub scores: Vec<(u64, u64)>,
    pub error: Option<String>,
}
