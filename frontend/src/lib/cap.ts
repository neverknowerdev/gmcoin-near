import { percent } from "./format";
import type { Settings, User } from "./types";

export interface CapStatus {
  /** verified users have no cap */
  capped: boolean;
  minted: bigint;
  cap: bigint;
  remaining: bigint;
  pct: number;
  reached: boolean;
}

/** The $1 cap: unverified users' lifetime minted rewards vs `unverified_cap`. */
export function capStatus(user: User | null, settings: Settings | null): CapStatus | null {
  if (!user || !settings) return null;
  if (user.verifiedHuman) {
    return { capped: false, minted: user.mintedUnverified, cap: settings.unverifiedCap, remaining: 0n, pct: 0, reached: false };
  }
  const cap = settings.unverifiedCap;
  const remaining = cap > user.mintedUnverified ? cap - user.mintedUnverified : 0n;
  return {
    capped: true,
    minted: user.mintedUnverified,
    cap,
    remaining,
    pct: percent(user.mintedUnverified, cap),
    reached: cap > 0n && user.mintedUnverified >= cap,
  };
}
