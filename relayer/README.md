# GM Coin gasless relayer (Cloudflare Worker)

Accepts NEP-366 `SignedDelegateAction`s (base64 borsh) from the frontend, wraps them in a transaction
signed and paid by the relayer account, and submits via `send_tx`. The core contract checks the user's
identity itself (predecessor = user); the relayer only decides whether to pay.

- `POST /relay` `{signed_delegate, turnstile_token}` -> `{ok:true, tx_hash}` or `{ok:false, error}` (400/403/429/502/503)
- `GET /health` -> `{ok, relayer, paused}`

## Drain protections (`src/policy.ts`, `src/limits.ts`)
- receiver must equal `CORE_CONTRACT_ID`; every action must be a `FunctionCall`
- methods limited to `register`, `start_x_link`, `verify_x_link`
- attached deposit must be exactly 0 on every action (the relayer would fund it, and a failed call refunds the user)
- gas per action and total <= `MAX_GAS_PER_ACTION` (default 300 TGas); <= 3 actions
- `max_block_height` must be in (current, current + 1000]
- Cloudflare Turnstile token required (skippable only if `TURNSTILE_SECRET` unset AND `ALLOW_NO_TURNSTILE="1"`)
- per-account (20/day) and per-IP (50/day) limits in KV `LIMITS`; estimated spend per hour
  (`EST_COST_YOCTO`, default 0.003 NEAR per relay) capped by `HOURLY_SPEND_CAP_YOCTO` (default 1 NEAR) -> 503; manual `PAUSED="1"`

Known limitation: nonce is read from RPC per request, so highly concurrent relays can collide on the relayer
key's nonce (the loser gets a 502 and can retry). Add more relayer keys or a Durable Object if this matters.

## Deploy
```
npm install
npm test && npm run typecheck
# edit wrangler.toml (names/ids there are placeholders)
npx wrangler kv namespace create LIMITS     # put the id in wrangler.toml
npx wrangler secret put RELAYER_PRIVATE_KEY # ed25519:...
npx wrangler secret put TURNSTILE_SECRET
npx wrangler deploy
```
The relayer account needs a full-access (or function-call) key and a NEAR balance.
