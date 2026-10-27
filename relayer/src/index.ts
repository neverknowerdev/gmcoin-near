import { KeyPairSigner, type SignedDelegate, actions, baseDecode, baseEncode, createTransaction, encodeTransaction } from "near-api-js";
import { decodeSignedDelegate } from "./decode";
import { DEFAULT_LIMITS, checkLimits, recordRelay, type KV } from "./limits";
import { DEFAULT_MAX_GAS, checkPolicy } from "./policy";

export interface Env {
  LIMITS: KV; CORE_CONTRACT_ID: string; RELAYER_ACCOUNT_ID: string; RELAYER_PRIVATE_KEY: string;
  NEAR_RPC_URL?: string; TURNSTILE_SECRET?: string; ALLOW_NO_TURNSTILE?: string; PAUSED?: string;
  ALLOWED_ORIGIN?: string; MAX_GAS_PER_ACTION?: string; RELAY_LIMIT_PER_ACCOUNT_DAY?: string;
  RELAY_LIMIT_PER_IP_DAY?: string; EST_COST_YOCTO?: string; HOURLY_SPEND_CAP_YOCTO?: string;
}

const cors = (env: Env) => ({
  "access-control-allow-origin": env.ALLOWED_ORIGIN || "*",
  "access-control-allow-methods": "GET,POST,OPTIONS",
  "access-control-allow-headers": "content-type",
});
const json = (env: Env, body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json", ...cors(env) } });
const fail = (env: Env, status: number, error: string) => json(env, { ok: false, error }, status);

async function rpc(env: Env, method: string, params: unknown): Promise<any> {
  const res = await fetch(env.NEAR_RPC_URL || "https://rpc.testnet.near.org", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: "relay", method, params }),
  });
  const j: any = await res.json();
  if (j.error) throw new Error(j.error.data?.toString?.() || j.error.message || "rpc error");
  return j.result;
}

async function verifyTurnstile(env: Env, token: unknown, ip: string): Promise<boolean> {
  if (!env.TURNSTILE_SECRET) return env.ALLOW_NO_TURNSTILE === "1";
  if (typeof token !== "string" || !token) return false;
  const res = await fetch("https://challenges.cloudflare.com/turnstile/v0/siteverify", {
    method: "POST", body: new URLSearchParams({ secret: env.TURNSTILE_SECRET, response: token, remoteip: ip }),
  });
  return ((await res.json()) as any).success === true;
}

/** Wraps the SignedDelegate in a tx signed by the relayer and submits it. Returns the tx hash. */
async function sendDelegate(env: Env, signedDelegate: SignedDelegate, block: any) {
  const signer = KeyPairSigner.fromSecretKey(env.RELAYER_PRIVATE_KEY as any);
  const pk = await signer.getPublicKey();
  const key = await rpc(env, "query", { request_type: "view_access_key", finality: "final", account_id: env.RELAYER_ACCOUNT_ID, public_key: pk.toString() });
  const action = actions.signedDelegate({ delegateAction: signedDelegate.delegateAction, signature: signedDelegate.signature });
  // Tx receiver is the delegate's sender (NEP-366).
  const tx = createTransaction(env.RELAYER_ACCOUNT_ID, pk, signedDelegate.delegateAction.senderId, BigInt(key.nonce) + 1n, [action], baseDecode(block.header.hash));
  const { txHash: hash, signedTransaction: signedTx } = await signer.signTransaction(tx);
  const b64 = btoa(String.fromCharCode(...encodeTransaction(signedTx)));
  const out = await rpc(env, "send_tx", { signed_tx_base64: b64, wait_until: "EXECUTED_OPTIMISTIC" });
  const status = out?.status;
  if (status && typeof status === "object" && "Failure" in status) throw new Error("transaction failed: " + JSON.stringify(status.Failure));
  return baseEncode(hash);
}

async function relay(req: Request, env: Env): Promise<Response> {
  const ip = req.headers.get("cf-connecting-ip") || "unknown";
  if (env.PAUSED === "1") return fail(env, 503, "relayer paused");
  let body: any;
  try { body = await req.json(); } catch { return fail(env, 400, "invalid json"); }
  if (typeof body?.signed_delegate !== "string") return fail(env, 400, "signed_delegate required");
  if (!(await verifyTurnstile(env, body.turnstile_token, ip))) return fail(env, 403, "turnstile verification failed");

  let dec;
  try { dec = decodeSignedDelegate(body.signed_delegate); } catch { return fail(env, 400, "cannot decode signed_delegate"); }
  const { delegate, signed } = dec;
  const cfg = { coreContractId: env.CORE_CONTRACT_ID, maxGasPerAction: env.MAX_GAS_PER_ACTION ? BigInt(env.MAX_GAS_PER_ACTION) : DEFAULT_MAX_GAS };
  // Static checks first (height-independent), so rejects never touch the RPC.
  const staticErr = checkPolicy(delegate, cfg, delegate.maxBlockHeight - 1n);
  if (staticErr) return fail(env, 403, staticErr);

  const lim = await checkLimits(env.LIMITS, delegate.senderId, ip, {
    perAccountDay: Number(env.RELAY_LIMIT_PER_ACCOUNT_DAY ?? DEFAULT_LIMITS.perAccountDay),
    perIpDay: Number(env.RELAY_LIMIT_PER_IP_DAY ?? DEFAULT_LIMITS.perIpDay),
    estCostYocto: BigInt(env.EST_COST_YOCTO ?? DEFAULT_LIMITS.estCostYocto),
    hourlySpendCapYocto: BigInt(env.HOURLY_SPEND_CAP_YOCTO ?? DEFAULT_LIMITS.hourlySpendCapYocto),
  });
  if (!lim.ok) return fail(env, lim.status, lim.error);

  try {
    const block = await rpc(env, "block", { finality: "final" });
    const heightErr = checkPolicy(delegate, cfg, BigInt(block.header.height));
    if (heightErr) return fail(env, 403, heightErr);
    // Count the relay before sending: a relayed call that fails still burns our gas.
    await recordRelay(env.LIMITS, delegate.senderId, ip);
    const txHash = await sendDelegate(env, signed, block);
    return json(env, { ok: true, tx_hash: txHash });
  } catch (e) {
    return fail(env, 502, (e as Error).message);
  }
}

export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    const url = new URL(req.url);
    if (req.method === "OPTIONS") return new Response(null, { status: 204, headers: cors(env) });
    if (req.method === "GET" && url.pathname === "/health")
      return json(env, { ok: true, relayer: env.RELAYER_ACCOUNT_ID, paused: env.PAUSED === "1" });
    if (req.method === "POST" && url.pathname === "/relay") return relay(req, env);
    return fail(env, 404, "not found");
  },
};
