//! GM core: users, X linking, daily scans through OutLayer, settlement and
//! weekly emission. The only minter of the GM token.
//!
//! Rules live in pure crates (`gm-scoring`, `epochs`, `caps`); methods here
//! check the caller, call a rule and save state.

use near_sdk::borsh::BorshSerialize;
use near_sdk::json_types::{U128, U64};
use near_sdk::store::{LookupMap, Vector};
use near_sdk::{env, near, require, AccountId, BorshStorageKey, Gas, NearToken, PanicOnDefault};

mod outlayer;
mod pipeline;
mod types;
mod users;
mod views;

pub use types::*;

const GAS_FOR_MIGRATE: Gas = Gas::from_tgas(20);

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    Users,
    ByAccount,
    ByX,
    Linked,
    PendingLinks,
    Days,
    Scores,
    Epochs,
    ActiveCounts,
    PendingMints,
    Leaderboards,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    owner: AccountId,
    token: AccountId,
    treasury: AccountId,
    outlayer: AccountId,
    config: Config,
    modules: Modules,
    paused: bool,

    // calendar
    genesis_ns: u64,
    day_ns: u64,
    days_per_epoch: u32,
    /// Staging only: added to the block time.
    clock_offset_ns: u64,

    // users
    users: LookupMap<UserId, User>,
    by_account: LookupMap<AccountId, UserId>,
    by_x: LookupMap<String, UserId>,
    /// Users with X linked, in link order; scans walk this list.
    linked: Vector<UserId>,
    pending_links: LookupMap<UserId, PendingLink>,
    next_user_id: UserId,
    user_count: u64,

    // pipeline
    days: LookupMap<DayIndex, Day>,
    /// (day, position) -> (user, points)
    scores: LookupMap<(DayIndex, u32), (UserId, u64)>,
    epochs: Vector<Epoch>,
    /// Users that reached `active_min_days` in an epoch.
    active_counts: LookupMap<EpochIndex, u64>,
    leaderboards: LookupMap<EpochIndex, Vec<(UserId, u64)>>,
    scan_day: DayIndex,
    settle_day: DayIndex,
    /// Rounding dust carried to the next settled day.
    dust: u128,
    /// Tips paid since the last settlement; they come out of the next day's pool.
    tips_unsettled: u128,
    tips: Counter,
    sponsor_spent: Counter,
    sponsor_registrations: Counter,
    pending_mints: LookupMap<u64, MintBatch>,
    /// Ids of mint batches waiting for a retry.
    failed_mints: Vec<u64>,
    next_batch_id: u64,
    /// GM core has asked the token to mint (rewards, treasury, tips).
    total_minted: u128,
}

#[near]
impl Contract {
    #[init]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        owner: AccountId,
        token: AccountId,
        treasury: AccountId,
        outlayer: AccountId,
        config: Config,
        day_length_ns: U64,
        days_per_epoch: u32,
        genesis_ns: Option<U64>,
    ) -> Self {
        Self::validate_config(&config);
        require!(day_length_ns.0 > 0 && days_per_epoch > 0, "bad calendar");
        let now = env::block_timestamp();
        let genesis = genesis_ns
            .map(|g| g.0)
            .unwrap_or(now / day_length_ns.0 * day_length_ns.0);
        let mut this = Self {
            owner,
            token,
            treasury,
            outlayer,
            config,
            modules: Modules::default(),
            paused: false,
            genesis_ns: genesis,
            day_ns: day_length_ns.0,
            days_per_epoch,
            clock_offset_ns: 0,
            users: LookupMap::new(StorageKey::Users),
            by_account: LookupMap::new(StorageKey::ByAccount),
            by_x: LookupMap::new(StorageKey::ByX),
            linked: Vector::new(StorageKey::Linked),
            pending_links: LookupMap::new(StorageKey::PendingLinks),
            next_user_id: 1,
            user_count: 0,
            days: LookupMap::new(StorageKey::Days),
            scores: LookupMap::new(StorageKey::Scores),
            epochs: Vector::new(StorageKey::Epochs),
            active_counts: LookupMap::new(StorageKey::ActiveCounts),
            leaderboards: LookupMap::new(StorageKey::Leaderboards),
            scan_day: 0,
            settle_day: 0,
            dust: 0,
            tips_unsettled: 0,
            tips: Counter::default(),
            sponsor_spent: Counter::default(),
            sponsor_registrations: Counter::default(),
            pending_mints: LookupMap::new(StorageKey::PendingMints),
            failed_mints: vec![],
            next_batch_id: 1,
            total_minted: 0,
        };
        let budget = this.config.emission.b0.0;
        this.epochs.push(Epoch {
            index: 0,
            start_day: 0,
            budget: U128(budget),
            daily_pool: U128(budget / days_per_epoch as u128),
            active_users: 0,
            total_points: 0,
            minted: U128(0),
            closed: false,
        });
        this
    }

    // ---- owner ----

    pub fn set_config(&mut self, config: Config) {
        self.assert_owner();
        Self::validate_config(&config);
        self.config = config;
    }

    /// Sets the OutLayer module sources (WASM URL + hash) and secrets profile.
    pub fn set_modules(&mut self, modules: Modules) {
        self.assert_owner();
        self.modules = modules;
    }

    pub fn set_outlayer(&mut self, outlayer: AccountId) {
        self.assert_owner();
        self.outlayer = outlayer;
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.assert_owner();
        self.paused = paused;
    }

    /// Marks a user as a verified human (World ID later), lifting the $1 cap.
    pub fn set_verified(&mut self, account_id: AccountId, verified: bool) {
        self.assert_owner();
        let id = self.user_id_of(&account_id);
        let mut user = self.users.get(&id).cloned().unwrap();
        user.verified_human = verified;
        self.users.insert(id, user);
    }

    /// Ownership moves to the timelock when it is turned on.
    pub fn set_owner(&mut self, owner: AccountId) {
        self.assert_owner();
        self.owner = owner;
    }

    /// Deploys new code (input bytes) on this account and calls `migrate`.
    pub fn upgrade(&mut self) -> near_sdk::Promise {
        self.assert_owner();
        let code = env::input().unwrap_or_else(|| env::panic_str("no code"));
        near_sdk::Promise::new(env::current_account_id())
            .deploy_contract(code)
            .function_call(
                "migrate",
                vec![],
                NearToken::from_yoctonear(0),
                GAS_FOR_MIGRATE,
            )
    }

    #[private]
    #[init(ignore_state)]
    pub fn migrate() -> Self {
        env::state_read().unwrap_or_else(|| env::panic_str("no state"))
    }

    /// Staging only: moves core's clock forward to cross day and epoch ends.
    #[cfg(feature = "staging")]
    pub fn advance_clock(&mut self, by_ns: U64) {
        self.assert_owner();
        self.clock_offset_ns += by_ns.0;
    }
}

impl Contract {
    pub(crate) fn assert_owner(&self) {
        require!(env::predecessor_account_id() == self.owner, "only owner");
    }

    pub(crate) fn assert_not_paused(&self) {
        require!(!self.paused, "paused");
    }

    fn validate_config(config: &Config) {
        if let Err(e) = config.emission.to_config().validate() {
            env::panic_str(e);
        }
        require!(
            config.emission.treasury_bps <= 5_000,
            "treasury share too high"
        );
        require!(
            config.scan_batch_size > 0 && config.scan_batch_size <= 100,
            "bad batch size"
        );
        require!(config.scan_max_in_flight > 0, "bad in-flight limit");
        require!(config.scan_max_attempts > 0, "bad attempts");
        require!(
            config.settle_chunk > 0 && config.settle_chunk <= 200,
            "bad settle chunk"
        );
        require!(config.emission.active_min_days > 0, "bad active days");
    }

    pub(crate) fn now(&self) -> u64 {
        env::block_timestamp() + self.clock_offset_ns
    }

    pub(crate) fn calendar(&self) -> epochs::Calendar {
        epochs::Calendar {
            genesis_ns: self.genesis_ns,
            day_ns: self.day_ns,
            days_per_epoch: self.days_per_epoch,
        }
    }

    pub(crate) fn today(&self) -> DayIndex {
        self.calendar().day_index(self.now())
    }

    pub(crate) fn user_id_of(&self, account: &AccountId) -> UserId {
        *self
            .by_account
            .get(account)
            .unwrap_or_else(|| env::panic_str("not registered"))
    }
}

#[cfg(test)]
mod tests;
