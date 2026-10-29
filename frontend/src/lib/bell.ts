import type { BellMethod } from "./calls";
import type { Status } from "./types";

export interface BellState {
  scan_tick: boolean;
  settle_day: boolean;
  close_epoch: boolean;
}

/** Which bell buttons are enabled. Everything is off when core is paused or status is unknown. */
export function bellState(s: Status | null): BellState {
  if (!s || s.paused) return { scan_tick: false, settle_day: false, close_epoch: false };
  return { scan_tick: s.scanDue, settle_day: s.settleDue, close_epoch: s.closeDue };
}

/** Next step for the "run pipeline" loop; epochs close first, then settle, then scan. */
export function nextBell(s: Status | null): BellMethod | null {
  if (!s || s.paused) return null;
  if (s.closeDue) return "close_epoch";
  if (s.settleDue) return "settle_day";
  if (s.scanDue) return "scan_tick";
  return null;
}

/** True when a tick changed anything observable; the loop stops otherwise. */
export function progressed(a: Status, b: Status): boolean {
  return (
    a.scanDay !== b.scanDay ||
    a.settleDay !== b.settleDay ||
    a.currentEpoch !== b.currentEpoch ||
    a.failedMints !== b.failedMints ||
    a.totalMinted !== b.totalMinted ||
    a.scanDue !== b.scanDue ||
    a.settleDue !== b.settleDue ||
    a.closeDue !== b.closeDue
  );
}

export function dueSummary(s: Status | null): string {
  if (!s) return "unknown";
  const due = [s.scanDue && "scan", s.settleDue && "settle", s.closeDue && "close"].filter(Boolean);
  return due.length ? due.join(", ") + " due" : "nothing due";
}

export interface Runner {
  getStatus(): Promise<Status | null>;
  call(m: BellMethod): Promise<{ ok: boolean; value?: unknown; error?: string }>;
  log(line: string): void;
  shouldStop?(): boolean;
}

/**
 * Calls ticks until nothing is due. Stops on error, when a tick returns false,
 * when status did not change after a tick, or after `max` calls.
 */
export async function runPipeline(r: Runner, max = 60): Promise<{ calls: number; reason: string }> {
  let calls = 0;
  let status = await r.getStatus();
  while (calls < max) {
    if (r.shouldStop?.()) return { calls, reason: "stopped" };
    const m = nextBell(status);
    if (!m) return { calls, reason: "nothing due" };
    r.log(`calling ${m}`);
    const res = await r.call(m);
    calls++;
    if (!res.ok) {
      r.log(`${m} failed: ${res.error ?? "unknown error"}`);
      return { calls, reason: "error" };
    }
    if (res.value === false) {
      r.log(`${m} returned false (nothing to do on chain)`);
      return { calls, reason: `${m} returned false` };
    }
    const after = await r.getStatus();
    if (!after) return { calls, reason: "status unavailable" };
    if (status && !progressed(status, after)) {
      r.log(`${m}: status unchanged (async callbacks may still be running)`);
      return { calls, reason: "no progress" };
    }
    status = after;
  }
  return { calls, reason: "call limit reached" };
}
