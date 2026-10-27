//! GM token: NEP-141 + NEP-148 + NEP-145 with
//! - a transfer fee (`fee_bps`, hard-capped at 5%) sent to the treasury,
//! - `mint_batch` callable only by the minter (core), idempotent per batch id,
//! - a daily mint cap, `max_supply` and a pause flag as backstops against a bug in core.

use near_contract_standards::fungible_token::events::FtMint;
use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::{
    FungibleToken, FungibleTokenCore, FungibleTokenResolver,
};
use near_contract_standards::storage_management::{
    StorageBalance, StorageBalanceBounds, StorageManagement,
};
use near_sdk::borsh::BorshSerialize;
use near_sdk::collections::LazyOption;
use near_sdk::json_types::U128;
use near_sdk::store::LookupSet;
use near_sdk::{
    assert_one_yocto, env, near, require, AccountId, BorshStorageKey, Gas, NearToken,
    PanicOnDefault, PromiseOrValue,
};

/// Fee can never be set above 5%.
pub const MAX_FEE_BPS: u16 = 500;
const DAY_NS: u64 = 86_400 * 1_000_000_000;
const GAS_FOR_MIGRATE: Gas = Gas::from_tgas(20);

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    Accounts,
    Metadata,
    UsedBatches,
    FeeExempt,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
    owner: AccountId,
    minter: AccountId,
    treasury: AccountId,
    fee_bps: u16,
    fee_exempt: LookupSet<AccountId>,
    max_supply: u128,
    daily_mint_cap: u128,
    minted_day: u64,
    minted_today: u128,
    used_batches: LookupSet<u64>,
    paused: bool,
}

#[near(serializers = [json])]
pub struct TokenConfig {
    pub owner: AccountId,
    pub minter: AccountId,
    pub treasury: AccountId,
    pub fee_bps: u16,
    pub max_supply: U128,
    pub daily_mint_cap: U128,
    pub minted_today: U128,
    pub paused: bool,
}

#[near]
impl Contract {
    #[init]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        owner: AccountId,
        minter: AccountId,
        treasury: AccountId,
        metadata: FungibleTokenMetadata,
        max_supply: U128,
        daily_mint_cap: U128,
        fee_bps: u16,
    ) -> Self {
        metadata.assert_valid();
        require!(fee_bps <= MAX_FEE_BPS, "fee above 5%");
        let mut this = Self {
            token: FungibleToken::new(StorageKey::Accounts),
            metadata: LazyOption::new(StorageKey::Metadata, Some(&metadata)),
            owner,
            minter,
            treasury: treasury.clone(),
            fee_bps,
            fee_exempt: LookupSet::new(StorageKey::FeeExempt),
            max_supply: max_supply.0,
            daily_mint_cap: daily_mint_cap.0,
            minted_day: 0,
            minted_today: 0,
            used_batches: LookupSet::new(StorageKey::UsedBatches),
            paused: false,
        };
        this.token.internal_register_account(&treasury);
        this.fee_exempt.insert(treasury);
        this
    }

    /// Mints a batch. Only the minter; a used `batch_id` is refused, so a
    /// retried batch never mints twice. Unregistered receivers are registered,
    /// paid from the attached deposit and then the contract's own balance.
    #[payable]
    pub fn mint_batch(&mut self, batch_id: u64, entries: Vec<(AccountId, U128)>) {
        require!(env::predecessor_account_id() == self.minter, "only minter");
        require!(!self.paused, "paused");
        require!(self.used_batches.insert(batch_id), "batch already minted");

        let total = entries
            .iter()
            .try_fold(0u128, |acc, (_, a)| acc.checked_add(a.0))
            .unwrap_or_else(|| env::panic_str("amount overflow"));
        require!(
            self.token
                .total_supply
                .checked_add(total)
                .is_some_and(|s| s <= self.max_supply),
            "max supply exceeded"
        );
        let day = env::block_timestamp() / DAY_NS;
        if self.minted_day != day {
            self.minted_day = day;
            self.minted_today = 0;
        }
        require!(
            self.minted_today + total <= self.daily_mint_cap,
            "daily mint cap exceeded"
        );
        self.minted_today += total;

        for (account, amount) in &entries {
            if amount.0 == 0 {
                continue;
            }
            if !self.token.accounts.contains_key(account) {
                self.token.internal_register_account(account);
            }
            self.token.internal_deposit(account, amount.0);
        }
        let events: Vec<FtMint> = entries
            .iter()
            .filter(|(_, a)| a.0 > 0)
            .map(|(owner_id, amount)| FtMint {
                owner_id,
                amount: *amount,
                memo: None,
            })
            .collect();
        if !events.is_empty() {
            FtMint::emit_many(&events);
        }
    }

    // ---- settings: owner or minter (core forwards owner actions) ----

    fn assert_admin(&self) {
        let p = env::predecessor_account_id();
        require!(p == self.owner || p == self.minter, "only owner or minter");
    }

    pub fn set_fee_bps(&mut self, fee_bps: u16) {
        self.assert_admin();
        require!(fee_bps <= MAX_FEE_BPS, "fee above 5%");
        self.fee_bps = fee_bps;
    }

    pub fn set_treasury(&mut self, treasury: AccountId) {
        self.assert_admin();
        if !self.token.accounts.contains_key(&treasury) {
            self.token.internal_register_account(&treasury);
        }
        self.fee_exempt.insert(treasury.clone());
        self.treasury = treasury;
    }

    pub fn set_minter(&mut self, minter: AccountId) {
        self.assert_admin();
        self.minter = minter;
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.assert_admin();
        self.paused = paused;
    }

    pub fn set_daily_mint_cap(&mut self, cap: U128) {
        self.assert_admin();
        self.daily_mint_cap = cap.0;
    }

    pub fn set_fee_exempt(&mut self, account_id: AccountId, exempt: bool) {
        self.assert_admin();
        if exempt {
            self.fee_exempt.insert(account_id);
        } else {
            self.fee_exempt.remove(&account_id);
        }
    }

    /// Ownership moves to the timelock when it is turned on.
    pub fn set_owner(&mut self, owner: AccountId) {
        require!(env::predecessor_account_id() == self.owner, "only owner");
        self.owner = owner;
    }

    /// Deploys new code (input bytes) on this account and calls `migrate`.
    pub fn upgrade(&mut self) -> near_sdk::Promise {
        require!(env::predecessor_account_id() == self.owner, "only owner");
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

    // ---- views ----

    pub fn get_config(&self) -> TokenConfig {
        let day = env::block_timestamp() / DAY_NS;
        TokenConfig {
            owner: self.owner.clone(),
            minter: self.minter.clone(),
            treasury: self.treasury.clone(),
            fee_bps: self.fee_bps,
            max_supply: U128(self.max_supply),
            daily_mint_cap: U128(self.daily_mint_cap),
            minted_today: U128(if self.minted_day == day {
                self.minted_today
            } else {
                0
            }),
            paused: self.paused,
        }
    }

    pub fn is_batch_used(&self, batch_id: u64) -> bool {
        self.used_batches.contains(&batch_id)
    }

    pub fn is_fee_exempt(&self, account_id: AccountId) -> bool {
        self.fee_exempt.contains(&account_id)
    }

    pub fn max_supply(&self) -> U128 {
        U128(self.max_supply)
    }

    /// Fee charged on a transfer of `amount` between the two accounts.
    pub fn fee_for(&self, sender_id: AccountId, receiver_id: AccountId, amount: U128) -> U128 {
        U128(self.fee(&sender_id, &receiver_id, amount.0))
    }
}

impl Contract {
    fn fee(&self, sender: &AccountId, receiver: &AccountId, amount: u128) -> u128 {
        if self.fee_bps == 0
            || self.fee_exempt.contains(sender)
            || self.fee_exempt.contains(receiver)
        {
            return 0;
        }
        amount * self.fee_bps as u128 / 10_000
    }

    /// Moves the fee to the treasury and returns the net amount.
    fn take_fee(&mut self, sender: &AccountId, receiver: &AccountId, amount: u128) -> u128 {
        require!(amount > 0, "The amount should be a positive number");
        let fee = self.fee(sender, receiver, amount);
        if fee > 0 {
            let treasury = self.treasury.clone();
            self.token
                .internal_transfer(sender, &treasury, fee, Some("fee".into()));
        }
        amount - fee
    }
}

#[near]
impl FungibleTokenCore for Contract {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        assert_one_yocto();
        let sender = env::predecessor_account_id();
        let net = self.take_fee(&sender, &receiver_id, amount.0);
        self.token
            .internal_transfer(&sender, &receiver_id, net, memo);
    }

    /// The receiver is told the net amount, so DEX deposits stay correct.
    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        let sender = env::predecessor_account_id();
        let net = self.take_fee(&sender, &receiver_id, amount.0);
        self.token
            .ft_transfer_call(receiver_id, U128(net), memo, msg)
    }

    fn ft_total_supply(&self) -> U128 {
        self.token.ft_total_supply()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.token.ft_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenResolver for Contract {
    #[private]
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
    ) -> U128 {
        self.token
            .ft_resolve_transfer(sender_id, receiver_id, amount)
    }
}

#[near]
impl StorageManagement for Contract {
    #[payable]
    fn storage_deposit(
        &mut self,
        account_id: Option<AccountId>,
        registration_only: Option<bool>,
    ) -> StorageBalance {
        self.token.storage_deposit(account_id, registration_only)
    }

    #[payable]
    fn storage_withdraw(&mut self, amount: Option<NearToken>) -> StorageBalance {
        self.token.storage_withdraw(amount)
    }

    #[payable]
    fn storage_unregister(&mut self, force: Option<bool>) -> bool {
        #[allow(deprecated)]
        if let Some((_, balance)) = self.token.internal_storage_unregister(force) {
            near_sdk::log!("Closed account with balance {}", balance);
            true
        } else {
            false
        }
    }

    fn storage_balance_bounds(&self) -> StorageBalanceBounds {
        self.token.storage_balance_bounds()
    }

    fn storage_balance_of(&self, account_id: AccountId) -> Option<StorageBalance> {
        self.token.storage_balance_of(account_id)
    }
}

#[near]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }
}

pub fn default_metadata() -> FungibleTokenMetadata {
    FungibleTokenMetadata {
        spec: FT_METADATA_SPEC.to_string(),
        name: "GM".to_string(),
        symbol: "GM".to_string(),
        icon: None,
        reference: None,
        reference_hash: None,
        decimals: 18,
    }
}

#[cfg(test)]
mod tests;
