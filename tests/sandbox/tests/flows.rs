#![allow(unused_must_use)]
//! End-to-end flows on a local NEAR node: register, link X, scan, settle and
//! close the epoch, plus the failure paths from the plan.

use near_workspaces::types::NearToken;
use sandbox_tests::*;
use serde_json::json;

fn u(v: &serde_json::Value) -> u128 {
    v.as_str().unwrap().parse().unwrap()
}

#[tokio::test]
async fn register_link_scan_settle_and_close_epoch() -> anyhow::Result<()> {
    let env = setup(json!({})).await?;
    let alice = env.user("alice").await?;
    let bob = env.user("bob").await?;
    let bell = env.user("bell").await?;
    let a = env.link(&alice, "1001").await?;
    let b = env.link(&bob, "1002").await?;

    let user = env
        .view(&env.core, "get_user", json!({ "account_id": alice.id() }))
        .await?;
    assert_eq!(user["x_id"], "1001");
    // core forwarded the code and account to the module, paid by alice.
    let reqs = env.view(&env.outlayer, "requests", json!({})).await?;
    let req = &reqs[0];
    assert_eq!(req["payer"], alice.id().to_string());
    assert!(req["input_data"]
        .as_str()
        .unwrap()
        .contains("alice.test.near"));
    assert!(req["secrets_ref"]
        .as_str()
        .unwrap()
        .contains("testnet-mock"));

    // Early ticks do nothing and pay no tip.
    assert_eq!(env.tick(&bell, "scan_tick").await?, json!(false));
    assert_eq!(env.tick(&bell, "settle_day").await?, json!(false));
    assert_eq!(env.balance(bell.id()).await?, 0);

    env.advance_days(1).await?;
    env.script(
        json!([{ "Output": json!({ "ok": true, "scores": [[a, 30], [b, 10]] }).to_string() }]),
    )
    .await?;
    assert_eq!(env.tick(&bell, "scan_tick").await?, json!(true));
    let day = env.view(&env.core, "get_day", json!({ "day": 0 })).await?;
    assert_eq!(day["status"], "Scanned");
    assert_eq!(day["total_points"], 40);
    let scan_req = env.view(&env.outlayer, "requests", json!({})).await?;
    let input: serde_json::Value =
        serde_json::from_str(scan_req[2]["input_data"].as_str().unwrap())?;
    assert_eq!(input["users"], json!([[a, "1001"], [b, "1002"]]));
    assert_eq!(
        input["until"].as_u64().unwrap() - input["since"].as_u64().unwrap(),
        600
    );

    assert_eq!(env.tick(&bell, "settle_day").await?, json!(true));
    let day = env.view(&env.core, "get_day", json!({ "day": 0 })).await?;
    assert_eq!(day["status"], "Settled");
    let users_pool = u(&day["users_pool"]);
    assert_eq!(u(&day["pool"]), 1_000 * GM);
    assert_eq!(u(&day["treasury"]), 30 * GM);
    assert_eq!(env.balance(env.treasury.id()).await?, 30 * GM);
    assert_eq!(
        env.balance(alice.id()).await?,
        (users_pool * 30 / 40).min(500 * GM)
    );
    assert_eq!(env.balance(bob.id()).await?, users_pool * 10 / 40);
    assert_eq!(env.balance(bell.id()).await?, 2 * GM, "two ticks did work");
    let user = env
        .view(&env.core, "get_user", json!({ "account_id": bob.id() }))
        .await?;
    assert_eq!(user["delivered"], user["accrued"]);

    // Run the rest of the week (empty days) and close the epoch.
    env.advance_days(6).await?;
    for _ in 0..6 {
        env.script(json!([{ "Output": json!({ "ok": true, "scores": [] }).to_string() }]))
            .await?;
        assert_eq!(env.tick(&bell, "scan_tick").await?, json!(true));
        assert_eq!(env.tick(&bell, "settle_day").await?, json!(true));
    }
    let closed: bool = alice
        .call(env.core.id(), "close_epoch")
        .args_json(json!({}))
        .gas(TICK_GAS)
        .transact()
        .await?
        .json()?;
    assert!(closed);
    let e1 = env
        .view(&env.core, "get_epoch", json!({ "epoch": 1 }))
        .await?;
    // Nobody was active 3 days: the budget follows the schedule, 7000 * 0.985.
    assert_eq!(u(&e1["budget"]), 6_895 * GM);
    let board = env
        .view(&env.core, "leaderboard", json!({ "epoch": 0 }))
        .await?;
    assert_eq!(board[0][0], alice.id().to_string());
    Ok(())
}

#[tokio::test]
async fn outlayer_failures_skip_the_day_and_never_stall() -> anyhow::Result<()> {
    let env = setup(json!({})).await?;
    let alice = env.user("alice").await?;
    let bell = env.user("bell").await?;
    env.link(&alice, "1").await?;
    env.advance_days(2).await?;
    // null (failed run), a panic in OutLayer, bad JSON: three strikes.
    env.script(json!(["Null", "Panic", { "Output": "{\"unexpected\":1}" }]))
        .await?;
    for _ in 0..3 {
        // The tick succeeds; its value is the batch result (false: the run failed).
        // The scripted OutLayer panic shows up as a failed inner receipt only.
        let r = bell
            .call(env.core.id(), "scan_tick")
            .args_json(json!({}))
            .gas(TICK_GAS)
            .transact()
            .await?;
        assert!(r.is_success());
        assert!(!r.json::<bool>()?);
    }
    let day0 = env.view(&env.core, "get_day", json!({ "day": 0 })).await?;
    assert_eq!(day0["status"], "Failed");
    assert_eq!(env.status().await?["scan_day"], 1);
    assert_eq!(env.tick(&bell, "settle_day").await?, json!(true));
    let day0 = env.view(&env.core, "get_day", json!({ "day": 0 })).await?;
    assert_eq!(day0["status"], "Skipped");
    assert_eq!(
        env.balance(alice.id()).await?,
        0,
        "a failed day is never minted"
    );

    // The next day works normally.
    env.script(json!([{ "Output": json!({ "ok": true, "scores": [[1, 5]] }).to_string() }]))
        .await?;
    assert_eq!(env.tick(&bell, "scan_tick").await?, json!(true));
    assert_eq!(env.tick(&bell, "settle_day").await?, json!(true));
    assert!(env.balance(alice.id()).await? > 0);
    Ok(())
}

#[tokio::test]
async fn paused_token_mints_retry_with_same_batch() -> anyhow::Result<()> {
    let env = setup(json!({ "tip_amount": "0" })).await?;
    let alice = env.user("alice").await?;
    let bell = env.user("bell").await?;
    env.link(&alice, "1").await?;
    env.advance_days(1).await?;
    env.script(json!([{ "Output": json!({ "ok": true, "scores": [[1, 5]] }).to_string() }]))
        .await?;
    env.tick(&bell, "scan_tick").await?;

    ok(env
        .owner
        .call(env.token.id(), "set_paused")
        .args_json(json!({ "paused": true }))
        .transact()
        .await?)?;
    // The mint fails inside a receipt; core records the batch for retry.
    let r = bell
        .call(env.core.id(), "settle_day")
        .args_json(json!({}))
        .gas(TICK_GAS)
        .transact()
        .await?;
    assert!(r.is_success());
    assert_eq!(env.status().await?["failed_mints"], 1);
    assert_eq!(env.balance(alice.id()).await?, 0);

    ok(env
        .owner
        .call(env.token.id(), "set_paused")
        .args_json(json!({ "paused": false }))
        .transact()
        .await?)?;
    env.tick(&bell, "settle_day").await?;
    assert_eq!(env.status().await?["failed_mints"], 0);
    let paid = env.balance(alice.id()).await?;
    assert!(paid > 0);
    let batch_used = env
        .view(&env.token, "is_batch_used", json!({ "batch_id": 1 }))
        .await?;
    assert_eq!(batch_used, json!(true));
    Ok(())
}

#[tokio::test]
async fn replayed_batch_id_mints_once() -> anyhow::Result<()> {
    let env = setup(json!({})).await?;
    let root = env.worker.root_account()?;
    // A token whose minter is a plain account, to call mint_batch directly.
    let t = sub(&root, "t2", 10)
        .await?
        .deploy(&wasm("gm_token"))
        .await?
        .into_result()?;
    let minter = env.user("minter").await?;
    ok(t.call("new")
        .args_json(json!({
            "owner": env.owner.id(), "minter": minter.id(), "treasury": env.treasury.id(),
            "metadata": { "spec": "ft-1.0.0", "name": "GM", "symbol": "GM", "decimals": 18 },
            "max_supply": (1000 * GM).to_string(), "daily_mint_cap": (1000 * GM).to_string(), "fee_bps": 0
        }))
        .transact()
        .await?)?;
    let mint = || {
        minter
            .call(t.id(), "mint_batch")
            .args_json(json!({ "batch_id": 7, "entries": [[minter.id(), (5 * GM).to_string()]] }))
            .transact()
    };
    assert!(mint().await?.is_success());
    assert!(mint().await?.is_failure());
    let bal: String = t
        .view("ft_balance_of")
        .args_json(json!({ "account_id": minter.id() }))
        .await?
        .json()?;
    assert_eq!(bal, (5 * GM).to_string());
    Ok(())
}

#[tokio::test]
async fn fee_on_transfer_call_tells_receiver_the_net_amount() -> anyhow::Result<()> {
    let env = setup(json!({})).await?;
    let alice = env.user("alice").await?;
    let bell = env.user("bell").await?;
    env.link(&alice, "1").await?;
    env.advance_days(1).await?;
    env.script(json!([{ "Output": json!({ "ok": true, "scores": [[1, 5]] }).to_string() }]))
        .await?;
    env.tick(&bell, "scan_tick").await?;
    env.tick(&bell, "settle_day").await?;
    let start = env.balance(alice.id()).await?;
    let treasury_start = env.balance(env.treasury.id()).await?;

    // The "DEX" (mock receiver) needs token storage.
    ok(alice
        .call(env.token.id(), "storage_deposit")
        .args_json(json!({ "account_id": env.outlayer.id() }))
        .deposit(NearToken::from_millinear(2))
        .transact()
        .await?)?;
    let amount = 100 * GM;
    ok(alice
        .call(env.token.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": env.outlayer.id(), "amount": amount.to_string(),
            // The receiver refunds 10 GM.
            "msg": (10 * GM).to_string()
        }))
        .deposit(NearToken::from_yoctonear(1))
        .gas(TICK_GAS)
        .transact()
        .await?)?;
    let fee = amount / 100;
    let received = env.view(&env.outlayer, "received", json!({})).await?;
    assert_eq!(
        u(&received[0][1]),
        amount - fee,
        "receiver sees the net amount"
    );
    assert_eq!(
        env.balance(env.outlayer.id()).await?,
        amount - fee - 10 * GM
    );
    assert_eq!(env.balance(alice.id()).await?, start - amount + 10 * GM);
    assert_eq!(env.balance(env.treasury.id()).await? - treasury_start, fee);

    // Plain transfer to a fee-exempt account pays no fee.
    ok(env
        .owner
        .call(env.token.id(), "set_fee_exempt")
        .args_json(json!({ "account_id": env.outlayer.id(), "exempt": true }))
        .transact()
        .await?)?;
    let before = env.balance(env.outlayer.id()).await?;
    ok(alice
        .call(env.token.id(), "ft_transfer")
        .args_json(json!({ "receiver_id": env.outlayer.id(), "amount": GM.to_string() }))
        .deposit(NearToken::from_yoctonear(1))
        .transact()
        .await?)?;
    assert_eq!(env.balance(env.outlayer.id()).await? - before, GM);
    Ok(())
}

#[tokio::test]
async fn storage_is_paid_and_refunded_and_sponsorship_is_capped() -> anyhow::Result<()> {
    let env = setup(json!({})).await?;
    let alice = env.user("alice").await?;
    let before = alice.view_account().await?.balance;
    env.register(&alice, true).await?;
    ok(alice
        .call(env.core.id(), "unregister")
        .args_json(json!({}))
        .transact()
        .await?)?;
    let after = alice.view_account().await?.balance;
    // Only gas is lost; the 0.01 NEAR deposit came back.
    assert!(
        before.as_yoctonear() - after.as_yoctonear() < NearToken::from_millinear(5).as_yoctonear()
    );

    // Two sponsored registrations a day, the third is refused.
    for name in ["s1", "s2"] {
        let a = env.user(name).await?;
        env.register(&a, false).await?;
    }
    let s3 = env.user("s3").await?;
    let r = s3
        .call(env.core.id(), "register")
        .args_json(json!({}))
        .transact()
        .await?;
    assert!(r.is_failure());
    Ok(())
}

#[tokio::test]
async fn sponsored_x_check_is_paid_by_core() -> anyhow::Result<()> {
    let env = setup(json!({})).await?;
    let s = env.user("sponsored").await?;
    let id = env.register(&s, false).await?;
    ok(s.call(env.core.id(), "start_x_link")
        .args_json(json!({}))
        .transact()
        .await?)?;
    env.script(
        json!([{ "Output": json!({ "ok": true, "x_id": "77", "handle": "s" }).to_string() }]),
    )
    .await?;
    let linked: bool = ok(s
        .call(env.core.id(), "verify_x_link")
        .args_json(json!({}))
        .gas(TICK_GAS)
        .transact()
        .await?)?
    .json()?;
    assert!(linked);
    let reqs = env.view(&env.outlayer, "requests", json!({})).await?;
    assert_eq!(reqs[0]["payer"], env.core.id().to_string());
    let user = env
        .view(
            &env.core,
            "get_user_by_id",
            json!({ "user_id": id.to_string() }),
        )
        .await?;
    assert_eq!(user["checks"]["used"], "1");
    Ok(())
}

#[tokio::test]
async fn core_upgrades_in_place_and_keeps_state() -> anyhow::Result<()> {
    let env = setup(json!({})).await?;
    let alice = env.user("alice").await?;
    env.link(&alice, "5").await?;
    ok(env
        .owner
        .call(env.core.id(), "upgrade")
        .args(wasm("gm_core_staging"))
        .gas(TICK_GAS)
        .transact()
        .await?)?;
    let user = env
        .view(&env.core, "get_user", json!({ "account_id": alice.id() }))
        .await?;
    assert_eq!(user["x_id"], "5");
    // Strangers cannot upgrade.
    let r = alice
        .call(env.core.id(), "upgrade")
        .args(wasm("gm_core"))
        .gas(TICK_GAS)
        .transact()
        .await?;
    assert!(r.is_failure());
    Ok(())
}

#[tokio::test]
async fn settles_many_users_in_chunks_within_gas() -> anyhow::Result<()> {
    let env = setup(json!({ "settle_chunk": 50, "scan_batch_size": 25 })).await?;
    let bell = env.user("bell").await?;
    let mut ids = vec![];
    for i in 0..60 {
        let a = env.user(&format!("u{i}")).await?;
        ids.push(env.link(&a, &format!("{}", 5000 + i)).await?);
    }
    env.advance_days(1).await?;
    for chunk in ids.chunks(25) {
        let scores: Vec<_> = chunk.iter().map(|id| json!([id, 3])).collect();
        env.script(json!([{ "Output": json!({ "ok": true, "scores": scores }).to_string() }]))
            .await?;
        let r = bell
            .call(env.core.id(), "scan_tick")
            .args_json(json!({}))
            .gas(TICK_GAS)
            .transact()
            .await?;
        let r = ok(r)?;
        println!(
            "scan_tick of {} users burnt {} TGas",
            chunk.len(),
            r.total_gas_burnt.as_tgas()
        );
    }
    let day = env.view(&env.core, "get_day", json!({ "day": 0 })).await?;
    assert_eq!(day["total_points"], 180);
    let mut rounds = 0;
    loop {
        let r = ok(bell
            .call(env.core.id(), "settle_day")
            .args_json(json!({}))
            .gas(TICK_GAS)
            .transact()
            .await?)?;
        println!("settle_day burnt {} TGas", r.total_gas_burnt.as_tgas());
        rounds += 1;
        let day = env.view(&env.core, "get_day", json!({ "day": 0 })).await?;
        if day["status"] == "Settled" {
            break;
        }
        assert!(rounds < 5);
    }
    assert_eq!(rounds, 2);
    let st = env.status().await?;
    assert_eq!(st["failed_mints"], 0);
    Ok(())
}
