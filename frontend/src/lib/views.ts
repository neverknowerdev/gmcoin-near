import { toBig } from "./format";
import type { Epoch, Estimate, LeaderRow, PendingLink, Settings, Status, User } from "./types";

type Obj = Record<string, unknown>;
const isObj = (v: unknown): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);
const num = (v: unknown, d = 0): number => (typeof v === "number" && Number.isFinite(v) ? v : d);
const big = (v: unknown, d = 0n): bigint => toBig(v) ?? d;
const str = (v: unknown): string | null => (typeof v === "string" ? v : null);

export function parseStatus(v: unknown): Status | null {
  if (!isObj(v)) return null;
  return {
    now: big(v.now),
    today: num(v.today),
    currentEpoch: num(v.current_epoch),
    budget: big(v.budget),
    dailyPool: big(v.daily_pool),
    scanDay: num(v.scan_day),
    settleDay: num(v.settle_day),
    scanDue: v.scan_due === true,
    settleDue: v.settle_due === true,
    closeDue: v.close_due === true,
    users: num(v.users),
    linked: num(v.linked),
    totalMinted: big(v.total_minted),
    paused: v.paused === true,
    failedMints: num(v.failed_mints),
  };
}

export function parseEpoch(v: unknown): Epoch | null {
  if (!isObj(v)) return null;
  return {
    index: num(v.index),
    startDay: num(v.start_day),
    budget: big(v.budget),
    dailyPool: big(v.daily_pool),
    activeUsers: num(v.active_users),
    totalPoints: num(v.total_points),
    minted: big(v.minted),
    closed: v.closed === true,
  };
}

export function parseEpochs(v: unknown): Epoch[] {
  if (!Array.isArray(v)) return [];
  return v.map(parseEpoch).filter((e): e is Epoch => e !== null);
}

export function parseUser(v: unknown): User | null {
  if (!isObj(v)) return null;
  return {
    id: String(v.id ?? ""),
    nearAccount: str(v.near_account) ?? "",
    xId: str(v.x_id),
    xHandle: str(v.x_handle),
    accrued: big(v.accrued),
    delivered: big(v.delivered),
    mintedUnverified: big(v.minted_unverified),
    verifiedHuman: v.verified_human === true,
    sponsored: v.sponsored === true,
    storageDeposit: big(v.storage_deposit),
    linkedAt: toBig(v.linked_at),
    lastScoredDay: typeof v.last_scored_day === "number" ? v.last_scored_day : null,
    lastDayPoints: num(v.last_day_points),
    epochPoints: num(v.epoch_points),
  };
}

export function parsePendingLink(v: unknown): PendingLink | null {
  if (!isObj(v) || typeof v.code !== "string") return null;
  return {
    code: v.code,
    createdAt: big(v.created_at),
    inFlight: v.in_flight === true,
    lastError: str(v.last_error),
  };
}

export function parseEstimate(v: unknown): Estimate | null {
  if (!isObj(v)) return null;
  return {
    day: typeof v.day === "number" ? v.day : null,
    points: num(v.points),
    dayPoints: num(v.day_points),
    estimate: big(v.estimate),
  };
}

export function parseSettings(v: unknown): Settings | null {
  if (!isObj(v)) return null;
  const cfg = isObj(v.config) ? v.config : {};
  const modules = isObj(v.modules) ? v.modules : {};
  const xv = isObj(modules.x_verify) ? modules.x_verify : null;
  return {
    owner: str(v.owner) ?? "",
    token: str(v.token) ?? "",
    treasury: str(v.treasury) ?? "",
    registerDeposit: big(cfg.register_deposit),
    unverifiedCap: big(cfg.unverified_cap),
    tipAmount: big(cfg.tip_amount),
    tipDailyLimit: big(cfg.tip_daily_limit),
    xVerifyDeposit: xv ? toBig(xv.deposit) : null,
    dayNs: big(v.day_ns, 86_400_000_000_000n),
    daysPerEpoch: num(v.days_per_epoch, 7),
  };
}

/** `leaderboard` returns [account, handle|null, points] tuples. */
export function parseLeaderboard(v: unknown): LeaderRow[] {
  if (!Array.isArray(v)) return [];
  const rows: LeaderRow[] = [];
  for (const r of v) {
    if (!Array.isArray(r) || typeof r[0] !== "string") continue;
    rows.push({ account: r[0], handle: str(r[1]), points: num(r[2]) });
  }
  return rows.sort((a, b) => b.points - a.points);
}

/** `ft_balance_of` returns a U128 string. */
export function parseBalance(v: unknown): bigint | null {
  return toBig(v);
}
