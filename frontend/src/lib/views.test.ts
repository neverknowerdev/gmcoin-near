import { describe, expect, it } from "vitest";
import { parseEpochs, parseEstimate, parseLeaderboard, parsePendingLink, parseSettings, parseStatus, parseUser } from "./views";

const status = {
  now: "1793284542846888514", today: 235, current_epoch: 3, budget: "729000000000000000000000",
  daily_pool: "104142857142857142857142", scan_day: 234, settle_day: 233,
  scan_due: true, settle_due: false, close_due: false, users: 4, linked: 2,
  total_minted: "5", paused: false, failed_mints: 1,
};

describe("parsers", () => {
  it("parses status with U64/U128 strings", () => {
    const s = parseStatus(status)!;
    expect(s.now).toBe(1793284542846888514n);
    expect(s.budget).toBe(729000000000000000000000n);
    expect(s.currentEpoch).toBe(3);
    expect(s.scanDue).toBe(true);
    expect(s.failedMints).toBe(1);
  });
  it("returns null for null and junk", () => {
    expect(parseStatus(null)).toBeNull();
    expect(parseStatus("x")).toBeNull();
    expect(parseUser(null)).toBeNull();
    expect(parsePendingLink(null)).toBeNull();
    expect(parseSettings(undefined)).toBeNull();
    expect(parseEstimate([])).toBeNull();
  });
  it("tolerates missing fields", () => {
    const s = parseStatus({})!;
    expect(s.budget).toBe(0n);
    expect(s.scanDue).toBe(false);
  });
  it("parses epochs and drops bad rows", () => {
    const e = parseEpochs([
      { index: 0, start_day: 0, budget: "10", daily_pool: "1", active_users: 0, total_points: 0, minted: "7", closed: true },
      null,
    ]);
    expect(e).toHaveLength(1);
    expect(e[0].minted).toBe(7n);
    expect(e[0].closed).toBe(true);
    expect(parseEpochs(null)).toEqual([]);
  });
  it("parses a user with options", () => {
    const u = parseUser({
      id: 3, near_account: "a.testnet", x_id: null, x_handle: null, accrued: "5", delivered: "4",
      minted_unverified: "4", verified_human: false, sponsored: true, storage_deposit: "0",
      created_at: "1", linked_at: null, last_scored_day: null, last_day_points: 0, epoch_points: 0,
    })!;
    expect(u.nearAccount).toBe("a.testnet");
    expect(u.xHandle).toBeNull();
    expect(u.linkedAt).toBeNull();
    expect(u.accrued).toBe(5n);
  });
  it("parses pending link", () => {
    const p = parsePendingLink({ code: "GM1abc", created_at: "9", in_flight: true, last_error: "nope" })!;
    expect(p).toMatchObject({ code: "GM1abc", inFlight: true, lastError: "nope" });
  });
  it("parses estimate", () => {
    expect(parseEstimate({ day: null, points: 0, day_points: 0, estimate: "0" })).toMatchObject({ day: null, estimate: 0n });
    expect(parseEstimate({ day: 4, points: 3, day_points: 9, estimate: "12" })).toMatchObject({ day: 4, estimate: 12n });
  });
  it("parses settings", () => {
    const s = parseSettings({
      owner: "o.testnet", token: "t", treasury: "tr", day_ns: "600000000000", days_per_epoch: 7,
      config: { register_deposit: "20000000000000000000000", unverified_cap: "100", tip_amount: "1", tip_daily_limit: "2" },
      modules: { x_verify: { deposit: "3000000000000000000000" }, x_scan: null },
    })!;
    expect(s.dayNs).toBe(600000000000n);
    expect(s.xVerifyDeposit).toBe(3000000000000000000000n);
    expect(parseSettings({ owner: "x" })!.xVerifyDeposit).toBeNull();
  });
  it("parses leaderboard tuples sorted by points", () => {
    const rows = parseLeaderboard([["a", null, 3], ["b", "bee", 9], "junk", [1, 2, 3]]);
    expect(rows.map((r) => r.account)).toEqual(["b", "a"]);
    expect(rows[0].handle).toBe("bee");
    expect(parseLeaderboard(null)).toEqual([]);
  });
});
