# Deploy runbook

The same steps run on testnet first, then mainnet. Account names are
placeholders until the main account is chosen (`gmcoin.near` is taken):
`<master>` owns `token.<master>`, `core.<master>`, `treasury.<master>` and
`relayer.<master>`.

## Tools

- Rust 1.97 with `wasm32-unknown-unknown` and `wasm32-wasip2`, `cargo-near`, `near-cli-rs`
- `python3` (with `cryptography` for OutLayer secrets), `jq`
- Keys in the near-cli legacy keychain (`~/.near-credentials/<network>/`). CI holds testnet keys only; mainnet deploys are run by hand.

## First deploy

1. **Build.** `./scripts/build.sh` (testnet) or `./scripts/build.sh --reproducible` (mainnet, needs Docker). Publish the printed sha256 with the release.
2. **Contracts.** Edit `deploy/<network>.json` (B0, MAX_SUPPLY, tip, $1 cap in GM), then

   ```bash
   python3 scripts/deploy.py --network testnet --master <master> --staging   # staging: advance_clock, 10-minute days
   python3 scripts/deploy.py --network mainnet --master <master>             # mainnet: refuses --staging
   ```

   This creates the subaccounts, deploys and initialises token (core as minter) and core, and makes core fee-exempt. Ids go to `deploy/<network>.<prefix><master>.out.json`.
3. **Token storage and exemptions.** Exempt DEX pools and other protocol accounts: `set_fee_exempt(account_id, true)` on the token (owner).
4. **OutLayer modules.**

   ```bash
   ./scripts/build-modules.sh
   python3 scripts/outlayer.py --network testnet --account <master> upload res/x-verify.wasm
   python3 scripts/outlayer.py --network testnet --account <master> upload res/x-scan.wasm
   python3 scripts/outlayer.py --network testnet --account <master> secrets res/x-scan.wasm \
     --profile <profile> --json '{"X_API_BASE":"...","X_API_KEY":"...","X_API_KEY_HEADER":"..."}'
   # same for res/x-verify.wasm; secrets are bound to each WASM hash
   ```

   Then call `set_modules` on core (owner) with each module's FastFS URL and hash and `secrets: {profile, account_id: <master>}`, or pass `--modules modules.json --secrets-profile <profile>` to `deploy.py`. `python3 scripts/outlayer.py estimate` prints the current run price for the configured limits (0.003 NEAR on testnet on 2026-10-27; the unused deposit is refunded).
5. **Fund core.** Core pays OutLayer deposits for scans and sponsored checks, and storage for sponsored users: keep a few NEAR above its storage stake. The token pays receivers' NEP-145 storage on first mint from its own balance (0.00125 NEAR each).
6. **Relayer.** `cd relayer && npx wrangler deploy`, set `RELAYER_PRIVATE_KEY` and `TURNSTILE_SECRET` with `wrangler secret put`, fund `relayer.<master>` with the float (about 5 NEAR) and start the top-up job.
7. **Frontend.** Cloudflare Pages with that network's contract ids.
8. **Smoke test.** Register, link the test X account, ring one tick by hand, then announce.

## Ticks

Anyone can call them; our backstop script calls them only when nobody has:

```bash
near contract call-function as-transaction core.<master> scan_tick json-args '{}' prepaid-gas '300.0 Tgas' attached-deposit '0 NEAR' sign-as <caller> network-config testnet sign-with-legacy-keychain send
# settle_day, close_epoch the same way; `status` (view) says what is due
```

## Upgrades

1. CI green, testnet deploy and end-to-end run (`.github/workflows/staging.yml`), then mainnet.
2. Core: owner calls `upgrade` with the new WASM as raw input (`near contract call-function as-transaction core.<master> upgrade file-args res/gm_core.wasm ...`); it deploys the code on itself and runs `migrate` in the same transaction. Every release ships a sandbox test that migrates the previous release's state.
3. A new module build has a new hash: bind its secrets first, then `set_modules`. Core keeps the old module until then.
4. Rollback: redeploy the previous release's WASM (kept as release assets) or point `set_modules` back to the previous hash. `set_paused(true)` on the token stops minting while we fix things; core's failed mint batches retry with the same batch id afterwards.

## Timelock (off for now)

Core and token take their owner as a setting (`set_owner`). Turning the timelock on is one ownership transfer to the timelock contract plus deleting the full-access keys of `token` and `core`.
