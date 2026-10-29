# GM Coin web frontend

Vite + React + TypeScript. Views use plain fetch JSON-RPC; transactions go through NEAR Wallet Selector (MyNearWallet, Meteor).

Pages (hash routes): `#/` home (connect, register, link X, balance), `#/leaderboard`, `#/budget`, `#/bell` (ring the GM bell), `#/lab` (staging only).

## Run

```bash
cd frontend
npm ci
cp .env.testnet .env.local     # or: npm run dev -- --mode testnet
npm run dev                    # http://localhost:5173
npm run build                  # production build into dist/
npm run build:testnet          # same, with .env.testnet
npm test                       # vitest unit tests
npm run typecheck
npm run e2e                    # Playwright smoke test against live testnet staging
```

`npm run e2e` builds with `--mode testnet`, serves it with `vite preview` and checks that Budget and Leaderboard render live data (read-only, no wallet). It uses the Chromium at `/opt/pw-browsers/chromium` (override with `CHROMIUM_PATH`); it never runs `playwright install`.

## Environment variables

| Variable | Meaning |
|---|---|
| `VITE_NETWORK` | `testnet` (default) or `mainnet` |
| `VITE_CORE` | core contract account (required) |
| `VITE_TOKEN` | GM token account (required) |
| `VITE_RPC` | RPC URL; defaults to `https://rpc.testnet.fastnear.com` / `https://rpc.mainnet.fastnear.com` |
| `VITE_RELAYER` | optional relayer URL passed to wallets that support meta-transactions |
| `VITE_STAGING` | `1` enables `/lab` (needs a core built with the `staging` feature) |

`.env.testnet` points at the staging deploy `mock-core.gmnear-dev-10272324.testnet`.

## Notes

- Amounts are yocto-GM (18 decimals); U64/U128 are JSON strings, parsed to `bigint` in `src/lib/views.ts`.
- Pure logic lives in `src/lib/` (formatting, call builders, bell state, X intent URL, chart geometry) and is unit tested.
- Wallet sign-in requests a function-call key for the core contract, so zero-deposit calls (ticks, `start_x_link`, sponsored `register`) sign without a prompt. Calls with a deposit always need wallet approval; redirect wallets return to the page with `?transactionHashes=`.
- `advance_clock` is owner-only on chain; the Lab shows the owner and warns when you are someone else.
