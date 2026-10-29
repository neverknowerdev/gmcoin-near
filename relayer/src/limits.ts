// Rate limits + spend-based auto-pause over a minimal KV interface.
export interface KV { get(key: string): Promise<string | null>; put(key: string, value: string, opts?: { expirationTtl?: number }): Promise<void> }
export interface LimitConfig { perAccountDay: number; perIpDay: number; estCostYocto: bigint; hourlySpendCapYocto: bigint }
export const DEFAULT_LIMITS: LimitConfig = {
  perAccountDay: 20, perIpDay: 50,
  estCostYocto: 3_000_000_000_000_000_000_000n,   // 0.003 NEAR
  hourlySpendCapYocto: 1_000_000_000_000_000_000_000_000n, // 1 NEAR
};
export type LimitResult = { ok: true } | { ok: false; status: 429 | 503; error: string };

const day = (t: number) => new Date(t).toISOString().slice(0, 10);
const hour = (t: number) => new Date(t).toISOString().slice(0, 13);

async function bump(kv: KV, key: string, ttl: number): Promise<void> {
  await kv.put(key, String(Number(await kv.get(key) ?? "0") + 1), { expirationTtl: ttl });
}

/** Checks all limits (no writes). Call recordRelay after a successful send. */
export async function checkLimits(kv: KV, sender: string, ip: string, cfg: LimitConfig, now = Date.now()): Promise<LimitResult> {
  const relays = Number(await kv.get(`spend:${hour(now)}`) ?? "0");
  if (BigInt(relays) * cfg.estCostYocto >= cfg.hourlySpendCapYocto) return { ok: false, status: 503, error: "relayer paused: hourly spend cap reached" };
  if (Number(await kv.get(`acct:${day(now)}:${sender}`) ?? "0") >= cfg.perAccountDay) return { ok: false, status: 429, error: "account daily limit reached" };
  if (Number(await kv.get(`ip:${day(now)}:${ip}`) ?? "0") >= cfg.perIpDay) return { ok: false, status: 429, error: "ip daily limit reached" };
  return { ok: true };
}

export async function recordRelay(kv: KV, sender: string, ip: string, now = Date.now()): Promise<void> {
  await bump(kv, `acct:${day(now)}:${sender}`, 172800);
  await bump(kv, `ip:${day(now)}:${ip}`, 172800);
  await bump(kv, `spend:${hour(now)}`, 7200);
}
