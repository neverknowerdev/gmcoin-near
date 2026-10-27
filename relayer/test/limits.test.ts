import { describe, expect, it } from "vitest";
import { DEFAULT_LIMITS, checkLimits, recordRelay } from "../src/limits";
import { MemKV } from "./helpers";

const t0 = Date.UTC(2026, 0, 1, 10, 0, 0);
describe("limits", () => {
  it("per-account limit", async () => {
    const kv = new MemKV();
    for (let i = 0; i < 20; i++) { expect((await checkLimits(kv, "a", `ip${i}`, DEFAULT_LIMITS, t0)).ok).toBe(true); await recordRelay(kv, "a", `ip${i}`, t0); }
    expect(await checkLimits(kv, "a", "fresh", DEFAULT_LIMITS, t0)).toMatchObject({ ok: false, status: 429 });
    expect((await checkLimits(kv, "b", "fresh", DEFAULT_LIMITS, t0)).ok).toBe(true);
  });
  it("per-ip limit", async () => {
    const kv = new MemKV();
    for (let i = 0; i < 50; i++) await recordRelay(kv, `u${i}`, "1.2.3.4", t0);
    expect(await checkLimits(kv, "new", "1.2.3.4", DEFAULT_LIMITS, t0)).toMatchObject({ ok: false, status: 429 });
  });
  it("resets on UTC day rollover", async () => {
    const kv = new MemKV();
    for (let i = 0; i < 20; i++) await recordRelay(kv, "a", "ip", t0);
    expect((await checkLimits(kv, "a", "ip", DEFAULT_LIMITS, t0)).ok).toBe(false);
    expect((await checkLimits(kv, "a", "ip", DEFAULT_LIMITS, Date.UTC(2026, 0, 2, 0, 0, 1))).ok).toBe(true);
  });
  it("hourly spend auto-pause returns 503 and clears next hour", async () => {
    const kv = new MemKV();
    const cfg = { ...DEFAULT_LIMITS, perAccountDay: 1e9, perIpDay: 1e9, hourlySpendCapYocto: DEFAULT_LIMITS.estCostYocto * 5n };
    for (let i = 0; i < 5; i++) await recordRelay(kv, `u${i}`, `i${i}`, t0);
    expect(await checkLimits(kv, "x", "y", cfg, t0)).toMatchObject({ ok: false, status: 503 });
    expect((await checkLimits(kv, "x", "y", cfg, t0 + 3_600_000)).ok).toBe(true);
  });
});
