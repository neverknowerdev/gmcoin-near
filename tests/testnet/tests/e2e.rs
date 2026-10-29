//! End-to-end on NEAR testnet against a staging deployment with the mock
//! OutLayer contract (scripts/deploy.py --staging --mock-outlayer):
//! register, link X, scan, settle, a full week and the epoch close, on real
//! testnet blocks and gas. Time is moved with core's staging `advance_clock`.
//!
//!   GM_TESTNET_DEPLOY=deploy/testnet.mock-<master>.out.json cargo test -p testnet-e2e -- --ignored --nocapture
//!
//! Signs with the master account's key from ~/.near-credentials/testnet.

use anyhow::{ensure, Result};
use near_workspaces::network::Testnet;
use near_workspaces::types::{Gas, NearToken};
use near_workspaces::{Account, AccountId, Worker};
use serde_json::{json, Value};

const GM: u128 = 1_000_000_000_000_000_000;
const TGAS300: Gas = Gas::from_tgas(300);

/// The public testnet RPC rate-limits bursts of requests.
fn rate_limited(e: &near_workspaces::error::Error) -> bool {
    let mut err: Option<&dyn std::error::Error> = Some(e);
    while let Some(x) = err {
        if x.to_string().contains("rate limit") {
            return true;
        }
        err = x.source();
    }
    false
}

async fn backoff(attempt: &mut u32) {
    *attempt += 1;
    eprintln!("testnet RPC rate limit, retry {attempt}");
    tokio::time::sleep(std::time::Duration::from_secs(2u64 << *attempt)).await;
}

struct Ctx {
    worker: Worker<Testnet>,
    master: Account,
    core: AccountId,
    token: AccountId,
    outlayer: AccountId,
    treasury: AccountId,
    day_ns: u64,
}

async fn ctx() -> Result<Ctx> {
    let path =
        std::env::var("GM_TESTNET_DEPLOY").expect("set GM_TESTNET_DEPLOY to a deploy/*.out.json");
    let path = format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"));
    let d: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let worker = near_workspaces::testnet().await?;
    let home = std::env::var("HOME")?;
    let master_id = d["master"].as_str().unwrap();
    // near-cli-rs legacy keychain file: { public_key, private_key, ... }
    let cred: Value = serde_json::from_str(&std::fs::read_to_string(format!(
        "{home}/.near-credentials/testnet/{master_id}.json"
    ))?)?;
    let sk: near_workspaces::types::SecretKey = cred["private_key"].as_str().unwrap().parse()?;
    let master = Account::from_secret_key(master_id.parse()?, sk, &worker);
    let id = |k: &str| -> AccountId { d[k].as_str().unwrap().parse().unwrap() };
    let mut c = Ctx {
        core: id("core"),
        token: id("token"),
        outlayer: id("outlayer"),
        treasury: id("treasury"),
        worker,
        master,
        day_ns: 0,
    };
    let settings = c.view(&c.core.clone(), "get_settings", json!({})).await?;
    c.day_ns = settings["day_ns"].as_str().unwrap().parse()?;
    Ok(c)
}

impl Ctx {
    async fn view(&self, contract: &AccountId, method: &str, args: Value) -> Result<Value> {
        let mut attempt = 0;
        loop {
            match self
                .worker
                .view(contract, method)
                .args_json(args.clone())
                .await
            {
                Ok(r) => return Ok(r.json()?),
                Err(e) if rate_limited(&e) && attempt < 6 => backoff(&mut attempt).await,
                Err(e) => return Err(e.into()),
            }
        }
    }

    async fn call(
        &self,
        who: &Account,
        contract: &AccountId,
        method: &str,
        args: Value,
        deposit: NearToken,
    ) -> Result<Value> {
        // A rate-limited RPC request is rejected before the transaction is
        // sent, so retrying it cannot execute the call twice.
        let mut attempt = 0;
        let r = loop {
            match who
                .call(contract, method)
                .args_json(args.clone())
                .deposit(deposit)
                .gas(TGAS300)
                .transact()
                .await
            {
                Ok(r) => break r,
                Err(e) if rate_limited(&e) && attempt < 6 => backoff(&mut attempt).await,
                Err(e) => return Err(e.into()),
            }
        };
        if let Some(f) = r.receipt_failures().first() {
            anyhow::bail!("{method} failed: {:?}", (*f).clone().into_result());
        }
        ensure!(r.is_success(), "{method} failed: {:?}", r.failures());
        Ok(r.json().unwrap_or(Value::Null))
    }

    async fn user(&self, tag: &str) -> Result<Account> {
        let name = format!(
            "e2e{}{tag}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs()
                % 100_000_000
        );
        Ok(self
            .master
            .create_subaccount(&name)
            .initial_balance(NearToken::from_millinear(500))
            .transact()
            .await?
            .into_result()?)
    }

    /// Replaces the mock's queue, so outputs left by an interrupted run
    /// cannot answer this run's requests.
    async fn script(&self, items: Value) -> Result<()> {
        let zero = NearToken::from_yoctonear(0);
        self.call(&self.master, &self.outlayer, "clear", json!({}), zero)
            .await?;
        self.call(
            &self.master,
            &self.outlayer,
            "push",
            json!({ "items": items }),
            NearToken::from_yoctonear(0),
        )
        .await?;
        Ok(())
    }

    async fn balance(&self, who: &AccountId) -> Result<u128> {
        let v = self
            .view(&self.token, "ft_balance_of", json!({ "account_id": who }))
            .await?;
        Ok(v.as_str().unwrap().parse()?)
    }

    async fn status(&self) -> Result<Value> {
        self.view(&self.core, "status", json!({})).await
    }

    /// Moves core's clock to the start of `day` + 1 second (staging only).
    async fn go_to_day(&self, day: u64) -> Result<()> {
        let st = self.status().await?;
        let s = self.view(&self.core, "get_settings", json!({})).await?;
        let genesis: u64 = s["genesis_ns"].as_str().unwrap().parse()?;
        let now: u64 = st["now"].as_str().unwrap().parse()?;
        let target = genesis + day * self.day_ns + 1_000_000_000;
        if target > now {
            self.call(
                &self.master,
                &self.core,
                "advance_clock",
                json!({ "by_ns": (target - now).to_string() }),
                NearToken::from_yoctonear(0),
            )
            .await?;
        }
        Ok(())
    }

    /// Runs scan + settle until the pipeline catches up with today.
    async fn catch_up(&self, bell: &Account, scores: impl Fn(u64) -> Value) -> Result<()> {
        for _ in 0..200 {
            let st = self.status().await?;
            if st["close_due"] == true {
                self.call(
                    bell,
                    &self.core,
                    "close_epoch",
                    json!({}),
                    NearToken::from_yoctonear(0),
                )
                .await?;
                continue;
            }
            if st["settle_due"] == true {
                self.call(
                    bell,
                    &self.core,
                    "settle_day",
                    json!({}),
                    NearToken::from_yoctonear(0),
                )
                .await?;
                continue;
            }
            if st["scan_due"] == true {
                let day = st["scan_day"].as_u64().unwrap();
                let d = self
                    .view(&self.core, "get_day", json!({ "day": day }))
                    .await?;
                let linked = st["linked"].as_u64().unwrap();
                if linked > 0 && (d.is_null() || d["status"] == "Scanning") {
                    self.script(json!([{ "Output": scores(day).to_string() }]))
                        .await?;
                }
                self.call(
                    bell,
                    &self.core,
                    "scan_tick",
                    json!({}),
                    NearToken::from_yoctonear(0),
                )
                .await?;
                continue;
            }
            return Ok(());
        }
        anyhow::bail!("pipeline did not catch up")
    }
}

#[tokio::test]
#[ignore = "needs a testnet deployment (GM_TESTNET_DEPLOY)"]
async fn testnet_full_week() -> Result<()> {
    let c = ctx().await?;
    let zero = NearToken::from_yoctonear(0);
    let bell = c.user("bell").await?;

    // Bring the pipeline to the present first (earlier runs may have left days behind).
    c.catch_up(&bell, |_| json!({ "ok": true, "scores": [] }))
        .await?;
    let st = c.status().await?;
    let start_day = st["today"].as_u64().unwrap();
    let epoch_before = st["current_epoch"].as_u64().unwrap();

    // Two users: one pays storage, one is sponsored (zero deposit).
    let alice = c.user("alice").await?;
    let bob = c.user("bob").await?;
    let a: String = serde_json::from_value(
        c.call(
            &alice,
            &c.core,
            "register",
            json!({}),
            NearToken::from_millinear(10),
        )
        .await?,
    )?;
    let b: String =
        serde_json::from_value(c.call(&bob, &c.core, "register", json!({}), zero).await?)?;
    let user_b = c
        .view(&c.core, "get_user", json!({ "account_id": bob.id() }))
        .await?;
    assert_eq!(user_b["sponsored"], true);

    for (acc, x) in [(&alice, format!("9{a}")), (&bob, format!("9{b}"))] {
        let code: String = serde_json::from_value(
            c.call(acc, &c.core, "start_x_link", json!({}), zero)
                .await?,
        )?;
        println!("{} link code {code}", acc.id());
        c.script(json!([{ "Output": json!({ "ok": true, "x_id": x, "handle": format!("h{x}"), "post_id": "1" }).to_string() }])).await?;
        let deposit = if acc.id() == alice.id() {
            NearToken::from_millinear(100)
        } else {
            zero
        };
        let linked = c
            .call(acc, &c.core, "verify_x_link", json!({}), deposit)
            .await?;
        assert_eq!(linked, json!(true));
    }

    // Next day: both score; alice 3x bob.
    c.go_to_day(start_day + 1).await?;
    let (ua, ub): (u64, u64) = (a.parse()?, b.parse()?);
    let tip_before = c.balance(bell.id()).await?;
    c.catch_up(
        &bell,
        |_| json!({ "ok": true, "scores": [[ua, 30], [ub, 10]] }),
    )
    .await?;
    let day = c
        .view(&c.core, "get_day", json!({ "day": start_day }))
        .await?;
    println!("day {start_day}: {day}");
    assert_eq!(day["status"], "Settled");
    let users_pool: u128 = day["users_pool"].as_str().unwrap().parse()?;
    let cap: u128 = c.view(&c.core, "get_settings", json!({})).await?["config"]["unverified_cap"]
        .as_str()
        .unwrap()
        .parse()?;
    assert_eq!(c.balance(alice.id()).await?, (users_pool * 3 / 4).min(cap));
    assert_eq!(c.balance(bob.id()).await?, (users_pool / 4).min(cap));
    assert!(
        c.balance(bell.id()).await? > tip_before,
        "tick caller earned GM tips"
    );
    assert!(c.balance(&c.treasury).await? > 0);

    // Run to the end of the epoch, the users scoring every day, and close it.
    let st = c.status().await?;
    let days = st["today"].as_u64().unwrap();
    let next_epoch_day = (epoch_before + 1) * 7;
    c.go_to_day(next_epoch_day.max(days) + 1).await?;
    c.catch_up(
        &bell,
        |_| json!({ "ok": true, "scores": [[ua, 3], [ub, 3]] }),
    )
    .await?;
    let st = c.status().await?;
    assert!(
        st["current_epoch"].as_u64().unwrap() > epoch_before,
        "epoch closed: {st}"
    );
    let closed = c
        .view(&c.core, "get_epoch", json!({ "epoch": epoch_before }))
        .await?;
    println!("closed epoch: {closed}");
    assert_eq!(closed["closed"], true);
    let board = c
        .view(&c.core, "leaderboard", json!({ "epoch": epoch_before }))
        .await?;
    println!("leaderboard: {board}");
    assert_eq!(st["failed_mints"], 0);
    println!("status: {st}");

    // A plain transfer pays the 1% fee to the treasury.
    let t0 = c.balance(&c.treasury).await?;
    c.call(
        &alice,
        &c.token,
        "storage_deposit",
        json!({ "account_id": bell.id() }),
        NearToken::from_millinear(2),
    )
    .await
    .ok();
    c.call(
        &alice,
        &c.token,
        "ft_transfer",
        json!({ "receiver_id": bell.id(), "amount": (10 * GM).to_string() }),
        NearToken::from_yoctonear(1),
    )
    .await?;
    assert_eq!(c.balance(&c.treasury).await? - t0, GM / 10);
    Ok(())
}
