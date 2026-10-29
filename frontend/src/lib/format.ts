export const GM_DECIMALS = 18;
export const NEAR_DECIMALS = 24;

/** Safe BigInt parse of a U64/U128 JSON string (or number). Returns null on junk. */
export function toBig(v: unknown): bigint | null {
  if (typeof v === "bigint") return v;
  if (typeof v === "number" && Number.isInteger(v)) return BigInt(v);
  if (typeof v === "string" && /^\d+$/.test(v)) return BigInt(v);
  return null;
}

function group(int: string): string {
  return int.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

/**
 * Formats an integer amount with `decimals` decimals, truncating (never
 * rounding up) to at most `maxFrac` fraction digits and trimming zeros.
 * Non-zero amounts that truncate to zero show as "<0.0001" style.
 */
export function formatUnits(
  amount: bigint | string | null | undefined,
  decimals = GM_DECIMALS,
  maxFrac = 4,
): string {
  const a = amount == null ? null : toBig(amount);
  if (a == null) return "-";
  const base = 10n ** BigInt(decimals);
  const int = a / base;
  const frac = (a % base).toString().padStart(decimals, "0").slice(0, maxFrac);
  const trimmed = frac.replace(/0+$/, "");
  if (a > 0n && int === 0n && trimmed === "") {
    return `<0.${"0".repeat(Math.max(maxFrac - 1, 0))}1`;
  }
  return trimmed ? `${group(int.toString())}.${trimmed}` : group(int.toString());
}

export const formatGm = (a: bigint | string | null | undefined, maxFrac = 4) =>
  formatUnits(a, GM_DECIMALS, maxFrac);

export const formatNear = (a: bigint | string | null | undefined, maxFrac = 4) =>
  formatUnits(a, NEAR_DECIMALS, maxFrac);

/** GM amount as a JS number, for chart geometry only. */
export function gmToNumber(a: bigint): number {
  return Number(a / 10n ** 12n) / 1e6;
}

/** Compact number for axes: 1.2k, 3.4M. */
export function compact(n: number): string {
  const abs = Math.abs(n);
  if (abs >= 1e9) return `${+(n / 1e9).toFixed(2)}B`;
  if (abs >= 1e6) return `${+(n / 1e6).toFixed(2)}M`;
  if (abs >= 1e3) return `${+(n / 1e3).toFixed(2)}k`;
  return `${+n.toFixed(2)}`;
}

export function shortAccount(id: string, max = 24): string {
  if (id.length <= max) return id;
  const keep = Math.floor((max - 1) / 2);
  return `${id.slice(0, keep)}…${id.slice(id.length - keep)}`;
}

/** Percent 0..100 of part/total, clamped; 0 when total is 0. */
export function percent(part: bigint, total: bigint): number {
  if (total <= 0n) return 0;
  const p = Number((part * 10000n) / total) / 100;
  return Math.min(100, Math.max(0, p));
}

export function formatNsDate(ns: bigint | null | undefined): string {
  if (ns == null) return "-";
  const d = new Date(Number(ns / 1_000_000n));
  return Number.isNaN(d.getTime()) ? "-" : d.toISOString().replace("T", " ").slice(0, 16) + " UTC";
}
