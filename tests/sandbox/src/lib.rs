//! Shared setup for the sandbox tests: a local NEAR node with the real token,
//! the staging build of core and the mock OutLayer contract.
//! Run `scripts/build.sh` first; the tests load WASM from `res/`.

#![allow(unused_must_use)]

use anyhow::Result;
use near_workspaces::network::Sandbox;
use near_workspaces::result::ExecutionFinalResult;
use near_workspaces::types::{Gas, NearToken};
use near_workspaces::{Account, Contract, Worker};
use serde_json::{json, Value};

pub const GM: u128 = 1_000_000_000_000_000_000;
pub const DAY_NS: u64 = 600 * 1_000_000_000;
pub const TICK_GAS: Gas = Gas::from_tgas(300);

pub fn wasm(name: &str) -> Vec<u8> {
    let path = format!("{}/../../res/{name}.wasm", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|_| panic!("{path} missing: run scripts/build.sh"))
}

pub fn core_config(overrides: Value) -> Value {
    let mut c = json!({
        "emission": {
            "b0": (7_000 * GM).to_string(),
            "decay_bps": 150,
            "step_cap_bps": 1000,
            "floor_bps": 1000,
            "ceiling_mult": 3,
            "baseline_active": 2,
            "max_supply": (1_000_000_000u128 * GM).to_string(),
            "treasury_bps": 300,
            "active_min_days": 3
        },
        "unverified_cap": (500 * GM).to_string(),
        "tip_amount": GM.to_string(),
        "tip_daily_limit": (100 * GM).to_string(),
        "register_deposit": NearToken::from_millinear(10).as_yoctonear().to_string(),
        "sponsored_storage_cost": NearToken::from_millinear(10).as_yoctonear().to_string(),
        "sponsor_checks_per_user_per_day": 3,
        "sponsor_daily_budget": NearToken::from_near(2).as_yoctonear().to_string(),
        "sponsor_registrations_per_day": 2,
        "sponsor_reclaim_after_ns": (7 * DAY_NS).to_string(),
        "scan_delay_ns": "0",
        "scan_batch_size": 25,
        "scan_max_in_flight": 4,
        "scan_max_attempts": 3,
        "settle_chunk": 50,
        "scoring": {
            "points_simple": 1, "points_hashtag": 3, "points_cashtag": 5, "points_per_like": 1,
            "max_hashtag_posts": 10, "max_cashtag_posts": 10, "max_simple_posts": 4294967295u32,
            "max_likes_per_post": 100, "spam_post_limit": 1000
        }
    });
    merge(&mut c, overrides);
    c
}

fn merge(a: &mut Value, b: Value) {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                merge(a.entry(k).or_insert(Value::Null), v);
            }
        }
        (a, b) => *a = b,
    }
}

pub fn module(deposit: NearToken) -> Value {
    json!({
        "source": { "WasmUrl": {
            "url": "https://test.fastfs.io/gm/outlayer.testnet/00.wasm",
            "hash": "ab".repeat(32),
            "build_target": "wasm32-wasip2"
        }},
        "max_instructions": "1000000000",
        "max_memory_mb": 128,
        "max_execution_seconds": "20",
        "deposit": deposit.as_yoctonear().to_string()
    })
}

pub struct Env {
    pub worker: Worker<Sandbox>,
    pub owner: Account,
    pub treasury: Account,
    pub token: Contract,
    pub core: Contract,
    pub outlayer: Contract,
}

pub async fn setup(config_overrides: Value) -> Result<Env> {
    let worker = near_workspaces::sandbox().await?;
    let root = worker.root_account()?;
    let owner = sub(&root, "owner", 50).await?;
    let treasury = sub(&root, "treasury", 5).await?;
    let token_acc = sub(&root, "token", 30).await?;
    let core_acc = sub(&root, "core", 60).await?;
    let ol_acc = sub(&root, "outlayer", 20).await?;

    let token = token_acc.deploy(&wasm("gm_token")).await?.into_result()?;
    let core = core_acc
        .deploy(&wasm("gm_core_staging"))
        .await?
        .into_result()?;
    let outlayer = ol_acc.deploy(&wasm("mock_outlayer")).await?.into_result()?;

    ok(outlayer.call("new").args_json(json!({})).transact().await?)?;
    ok(token
        .call("new")
        .args_json(json!({
            "owner": owner.id(),
            "minter": core.id(),
            "treasury": treasury.id(),
            "metadata": { "spec": "ft-1.0.0", "name": "GM", "symbol": "GM", "decimals": 18 },
            "max_supply": (1_000_000_000u128 * GM).to_string(),
            "daily_mint_cap": (100_000 * GM).to_string(),
            "fee_bps": 100
        }))
        .transact()
        .await?)?;
    ok(core
        .call("new")
        .args_json(json!({
            "owner": owner.id(),
            "token": token.id(),
            "treasury": treasury.id(),
            "outlayer": outlayer.id(),
            "config": core_config(config_overrides),
            "day_length_ns": DAY_NS.to_string(),
            "days_per_epoch": 7,
        }))
        .transact()
        .await?)?;
    ok(owner
        .call(core.id(), "set_modules")
        .args_json(json!({ "modules": {
            "x_verify": module(NearToken::from_millinear(100)),
            "x_scan": module(NearToken::from_millinear(100)),
            "secrets": { "profile": "testnet-mock", "account_id": owner.id() }
        }}))
        .transact()
        .await?)?;
    Ok(Env {
        worker,
        owner,
        treasury,
        token,
        core,
        outlayer,
    })
}

pub async fn sub(root: &Account, name: &str, near: u128) -> Result<Account> {
    Ok(root
        .create_subaccount(name)
        .initial_balance(NearToken::from_near(near))
        .transact()
        .await?
        .into_result()?)
}

pub fn ok(r: ExecutionFinalResult) -> Result<ExecutionFinalResult> {
    if r.is_failure() {
        anyhow::bail!("transaction failed: {:#?}", r.failures());
    }
    if let Some(f) = r.receipt_failures().first() {
        anyhow::bail!("receipt failed: {:#?}", f);
    }
    Ok(r)
}

impl Env {
    pub async fn user(&self, name: &str) -> Result<Account> {
        sub(&self.worker.root_account()?, name, 10).await
    }

    pub async fn script(&self, items: Value) -> Result<()> {
        ok(self
            .owner
            .call(self.outlayer.id(), "push")
            .args_json(json!({ "items": items }))
            .transact()
            .await?)?;
        Ok(())
    }

    pub async fn view(&self, c: &Contract, method: &str, args: Value) -> Result<Value> {
        Ok(c.view(method).args_json(args).await?.json()?)
    }

    pub async fn balance(&self, who: &near_workspaces::AccountId) -> Result<u128> {
        let v = self
            .view(&self.token, "ft_balance_of", json!({ "account_id": who }))
            .await?;
        Ok(v.as_str().unwrap().parse()?)
    }

    pub async fn status(&self) -> Result<Value> {
        self.view(&self.core, "status", json!({})).await
    }

    pub async fn advance_days(&self, days: u64) -> Result<()> {
        ok(self
            .owner
            .call(self.core.id(), "advance_clock")
            .args_json(json!({ "by_ns": (days * DAY_NS).to_string() }))
            .transact()
            .await?)?;
        Ok(())
    }

    /// Calls a tick as `caller` and returns its JSON result.
    pub async fn tick(&self, caller: &Account, method: &str) -> Result<Value> {
        let r = caller
            .call(self.core.id(), method)
            .args_json(json!({}))
            .gas(TICK_GAS)
            .transact()
            .await?;
        let r = ok(r)?;
        Ok(r.json().unwrap_or(Value::Null))
    }

    pub async fn register(&self, user: &Account, paid: bool) -> Result<u64> {
        let mut call = user.call(self.core.id(), "register").args_json(json!({}));
        if paid {
            call = call.deposit(NearToken::from_millinear(10));
        }
        let id: String = ok(call.transact().await?)?.json()?;
        Ok(id.parse()?)
    }

    /// register + start_x_link + scripted x-verify success + verify_x_link.
    pub async fn link(&self, user: &Account, x_id: &str) -> Result<u64> {
        let id = self.register(user, true).await?;
        let code: String = ok(user
            .call(self.core.id(), "start_x_link")
            .args_json(json!({}))
            .transact()
            .await?)?
        .json()?;
        self.script(json!([{ "Output": json!({
            "ok": true, "x_id": x_id, "handle": format!("user{x_id}"), "post_id": "1"
        }).to_string() }]))
            .await?;
        let linked: bool = ok(user
            .call(self.core.id(), "verify_x_link")
            .args_json(json!({}))
            .deposit(NearToken::from_millinear(100))
            .gas(TICK_GAS)
            .transact()
            .await?)?
        .json()?;
        anyhow::ensure!(linked, "link failed for {} ({code})", user.id());
        Ok(id)
    }
}
