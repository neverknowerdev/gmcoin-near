import { describe, expect, it } from "vitest";
import { bellState, nextBell, progressed, runPipeline } from "./bell";
import type { Status } from "./types";

const base: Status = {
  now: 0n, today: 5, currentEpoch: 0, budget: 0n, dailyPool: 0n, scanDay: 3, settleDay: 3,
  scanDue: false, settleDue: false, closeDue: false, users: 0, linked: 0, totalMinted: 0n, paused: false, failedMints: 0,
};

describe("bell state", () => {
  it("is all disabled without status or when paused", () => {
    expect(bellState(null)).toEqual({ scan_tick: false, settle_day: false, close_epoch: false });
    expect(bellState({ ...base, scanDue: true, paused: true }).scan_tick).toBe(false);
  });
  it("mirrors the due flags", () => {
    expect(bellState({ ...base, scanDue: true, closeDue: true })).toEqual({ scan_tick: true, settle_day: false, close_epoch: true });
  });
  it("picks close, then settle, then scan", () => {
    expect(nextBell({ ...base, scanDue: true, settleDue: true, closeDue: true })).toBe("close_epoch");
    expect(nextBell({ ...base, scanDue: true, settleDue: true })).toBe("settle_day");
    expect(nextBell({ ...base, scanDue: true })).toBe("scan_tick");
    expect(nextBell(base)).toBeNull();
    expect(nextBell(null)).toBeNull();
  });
  it("detects progress", () => {
    expect(progressed(base, { ...base })).toBe(false);
    expect(progressed(base, { ...base, scanDay: 4 })).toBe(true);
  });
});

describe("runPipeline", () => {
  it("runs ticks until nothing is due", async () => {
    let s: Status = { ...base, scanDue: true };
    const calls: string[] = [];
    const res = await runPipeline({
      getStatus: async () => s,
      call: async (m) => {
        calls.push(m);
        s = m === "scan_tick" ? { ...s, scanDay: s.scanDay + 1, scanDue: false, settleDue: true } : { ...s, settleDay: s.settleDay + 1, settleDue: false };
        return { ok: true, value: true };
      },
      log: () => {},
    });
    expect(calls).toEqual(["scan_tick", "settle_day"]);
    expect(res.reason).toBe("nothing due");
  });
  it("stops on error, false return, and no progress", async () => {
    const mk = (r: { ok: boolean; value?: unknown }) =>
      runPipeline({ getStatus: async () => ({ ...base, scanDue: true }), call: async () => r, log: () => {} });
    expect((await mk({ ok: false })).reason).toBe("error");
    expect((await mk({ ok: true, value: false })).reason).toMatch(/returned false/);
    const np = await mk({ ok: true, value: true });
    expect(np).toEqual({ calls: 1, reason: "no progress" });
  });
  it("respects the call limit", async () => {
    let n = 0;
    const res = await runPipeline(
      { getStatus: async () => ({ ...base, scanDue: true, scanDay: n }), call: async () => { n++; return { ok: true }; }, log: () => {} },
      3,
    );
    expect(res).toEqual({ calls: 3, reason: "call limit reached" });
  });
});
