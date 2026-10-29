import { KeyPair } from "near-api-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import worker, { type Env } from "../src/index";
import { MemKV, fc, makeDelegate } from "./helpers";
import { actions } from "near-api-js";

let calls: { url: string; method?: string; body?: any }[];
let env: Env;

beforeEach(() => {
  calls = [];
  env = {
    LIMITS: new MemKV(), CORE_CONTRACT_ID: "core.gmcoin.testnet", RELAYER_ACCOUNT_ID: "relayer.testnet",
    RELAYER_PRIVATE_KEY: KeyPair.fromRandom("ed25519").toString(), TURNSTILE_SECRET: "s", NEAR_RPC_URL: "https://rpc.test",
  };
  vi.stubGlobal("fetch", async (url: string, init: any) => {
    const body = init?.body;
    calls.push({ url, body });
    if (url.includes("turnstile")) return Response.json({ success: String(body).includes("response=good") });
    const m = JSON.parse(body).method;
    if (m === "block") return Response.json({ result: { header: { height: 1000, hash: "11111111111111111111111111111111" } } });
    if (m === "query") return Response.json({ result: { nonce: 7 } });
    if (m === "send_tx") return Response.json({ result: { status: { SuccessValue: "" } } });
    return Response.json({ error: { message: "unexpected" } });
  });
});
afterEach(() => vi.unstubAllGlobals());

const rpcCalls = () => calls.filter((c) => c.url === "https://rpc.test");
const post = (body: unknown) => worker.fetch(new Request("https://r/relay", { method: "POST", body: JSON.stringify(body), headers: { "cf-connecting-ip": "9.9.9.9" } }), env);

describe("handler", () => {
  it("health", async () => {
    const r = await worker.fetch(new Request("https://r/health"), { ...env, PAUSED: "1" });
    expect(await r.json()).toEqual({ ok: true, relayer: "relayer.testnet", paused: true });
  });
  it("rejects bad turnstile without RPC", async () => {
    const r = await post({ signed_delegate: await makeDelegate([fc("register")]), turnstile_token: "bad" });
    expect(r.status).toBe(403); expect(rpcCalls()).toHaveLength(0);
  });
  it("rejects policy violation without RPC", async () => {
    const r = await post({ signed_delegate: await makeDelegate([actions.transfer(1n)]), turnstile_token: "good" });
    expect(r.status).toBe(403); expect(((await r.json()) as any).ok).toBe(false); expect(rpcCalls()).toHaveLength(0);
  });
  it("rejects when paused without RPC", async () => {
    env.PAUSED = "1";
    const r = await post({ signed_delegate: await makeDelegate([fc("register")]), turnstile_token: "good" });
    expect(r.status).toBe(503); expect(rpcCalls()).toHaveLength(0);
  });
  it("allows no turnstile only with explicit flag", async () => {
    delete env.TURNSTILE_SECRET;
    const sd = await makeDelegate([fc("register")]);
    expect((await post({ signed_delegate: sd })).status).toBe(403);
    env.ALLOW_NO_TURNSTILE = "1";
    expect((await post({ signed_delegate: sd })).status).toBe(200);
  });
  it("sends a transaction for an accepted request", async () => {
    const r = await post({ signed_delegate: await makeDelegate([fc("register")]), turnstile_token: "good" });
    const j: any = await r.json();
    expect(r.status).toBe(200); expect(j.ok).toBe(true); expect(j.tx_hash).toBeTruthy();
    expect(rpcCalls().map((c) => JSON.parse(c.body).method)).toContain("send_tx");
  });
});
