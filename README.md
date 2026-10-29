# GM Coin on NEAR

Say GM on X, earn GM. Everything runs on NEAR: a NEP-141 token, a core
contract with users, X linking, points, a fixed weekly budget and minting, and
X checks through [OutLayer](https://outlayer.fastnear.com) modules running in a TEE.

## How it works

- **Link X.** `register`, then `start_x_link` gives a code (`GM` + offset + 10
  characters of your account's hash + 2 random). Post it on X and call
  `verify_x_link`: the `x-verify` OutLayer job finds the post by search (or
  reads the link you paste) and core links that X account to you.
- **Score.** Each day, the `x-scan` job fetches linked users' GM posts in
  batches of 25 and scores them with v1's rules: `gm` 1, `#gm` 3, `$gm` 5,
  +1 per like, daily caps (see `crates/gm-scoring`).
- **Fixed weekly supply.** Each epoch has a budget; each day mints
  `budget / 7` split by points, 3% to the treasury out of the pool. At
  `close_epoch` the next budget follows `B0 * 0.985^n`, scaled by
  `clamp(sqrt(active / baseline), 0.5, 2)`, moving at most 10% a week, with a
  floor of 10% of B0, a ceiling of 3x the 4-week average and `MAX_SUPPLY` in
  the token. Price is never an input (`crates/epochs`).
- **Ring the GM bell.** NEAR has no cron. `scan_tick`, `settle_day` and
  `close_epoch` are open to anyone, do one step only when it is due, and tip
  the caller in GM. An early or repeated call does nothing.
- **Gasless start.** A small relayer (`relayer/`) pays gas for `register`,
  `start_x_link` and `verify_x_link` only, with zero attached deposit; core
  pays storage and X checks for those users under its own daily caps.
- **$1 cap.** Until World ID lands, unverified users' lifetime minted rewards
  are capped at a fixed GM amount.

## Layout

| Path | What |
|---|---|
| `contracts/token` | GM NEP-141: transfer fee to treasury (max 5%), `mint_batch` by core only, idempotent batch ids, daily mint cap, `max_supply`, pause |
| `contracts/core` | Users, X linking, ticks, settlement, epochs, views, owner settings; `staging` feature adds `advance_clock` |
| `contracts/mock-outlayer` | Test double of OutLayer's `request_execution` with scripted outputs |
| `crates/gm-scoring` | Scoring rules and link codes (pure) |
| `crates/epochs` | Emission math, payouts, caps, calendar (pure) |
| `crates/caps` | Sponsored-call and tip caps (pure) |
| `crates/x-client` | CachedXApi client and the x-verify / x-scan job logic behind an `HttpClient` trait |
| `modules/` | The two OutLayer modules (WASI P2 binaries) |
| `relayer/` | Cloudflare Worker for NEP-366 meta-transactions |
| `tests/sandbox` | near-workspaces tests on a local node |
| `tests/modules` | The shipped module WASM under wasmtime + wiremock |
| `tests/testnet` | End-to-end week on testnet |
| `scripts/` | Build, deploy and OutLayer helpers |
| `deploy/` | Network parameters (B0, MAX_SUPPLY and tip size are placeholders until decided) |

## Build and test

```bash
cargo test --workspace --exclude sandbox-tests --exclude module-tests --exclude testnet-e2e  # unit + contract
./scripts/build.sh            # contract WASM into res/ (needs cargo-near)
cargo test -p sandbox-tests   # local NEAR node
./scripts/build-modules.sh && cargo test -p module-tests   # needs wasmtime
(cd relayer && npm ci && npm test)
```

Testnet: see [docs/DEPLOY.md](docs/DEPLOY.md).

## NEAR HACKATHON

Built for [NEAR HACKATHON](https://nearhackathon.com) (2026-11-01 to 2026-11-14;
code allowed from the 2026-10-19 announcement, rule Q-001). Every commit on this
branch is hackathon work: the repository started empty on 2026-10-27
(`700abc2`, README only). Full diff: `main...near-hackathon-build`.

- **Existing product (Q-002).** GM Coin v1 is an ERC-20 on Base with its own
  contracts and backend. None of its code is used here; this repo reuses v1's
  rules (scoring points, daily caps, link-code format, X search query shape),
  rewritten in Rust for NEAR. The weekly budget emission, the tick design, the
  OutLayer modules and the relayer are new.
- **Libraries (Q-005).** near-sdk and near-contract-standards 5.30,
  near-workspaces 0.24, cargo-near, wasi-http-client, serde, proptest,
  wasmtime + wiremock (tests), near-api-js 7 and Cloudflare Workers (relayer),
  vitest. No pre-hackathon code of our own.
- **AI code (Q-005).** Most code was written with Claude Code and reviewed by
  the team.
