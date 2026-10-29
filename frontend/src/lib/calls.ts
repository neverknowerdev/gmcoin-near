/** Pure builders for contract calls and the helpers around them. */

export const TGAS = 10n ** 12n;
export const GAS_300 = 300n * TGAS;
export const GAS_DEFAULT = 30n * TGAS;

export interface CallSpec {
  receiver: string;
  method: string;
  args: Record<string, unknown>;
  gas: bigint;
  /** yoctoNEAR */
  deposit: bigint;
}

export const register = (core: string, payDeposit: bigint | null): CallSpec => ({
  receiver: core,
  method: "register",
  args: {},
  gas: GAS_DEFAULT,
  deposit: payDeposit ?? 0n,
});

export const startXLink = (core: string): CallSpec => ({
  receiver: core,
  method: "start_x_link",
  args: {},
  gas: GAS_DEFAULT,
  deposit: 0n,
});

export const verifyXLink = (core: string, postUrl: string, payDeposit: bigint | null): CallSpec => {
  const url = postUrl.trim();
  return {
    receiver: core,
    method: "verify_x_link",
    args: { post_url: url === "" ? null : url },
    gas: GAS_300,
    deposit: payDeposit ?? 0n,
  };
}

export type BellMethod = "scan_tick" | "settle_day" | "close_epoch";

export const bellCall = (core: string, method: BellMethod): CallSpec => ({
  receiver: core,
  method,
  args: {},
  gas: GAS_300,
  deposit: 0n,
});

export const advanceClock = (core: string, byNs: bigint): CallSpec => ({
  receiver: core,
  method: "advance_clock",
  args: { by_ns: byNs.toString() },
  gas: GAS_DEFAULT,
  deposit: 0n,
});

/** Decodes a transaction's SuccessValue (base64 JSON) from a final outcome, or undefined. */
export function decodeSuccessValue(outcome: unknown): unknown {
  const status = (outcome as any)?.status;
  const b64: unknown = status?.SuccessValue;
  if (typeof b64 !== "string" || b64 === "") return undefined;
  try {
    return JSON.parse(atob(b64));
  } catch {
    return undefined;
  }
}

/** Failure message from a final outcome, if any. */
export function outcomeFailure(outcome: unknown): string | null {
  const f = (outcome as any)?.status?.Failure;
  if (!f) return null;
  const msg =
    f?.ActionError?.kind?.FunctionCallError?.ExecutionError ?? JSON.stringify(f);
  return String(msg);
}

export function txHash(outcome: unknown): string | null {
  const h = (outcome as any)?.transaction?.hash ?? (outcome as any)?.transaction_outcome?.id;
  return typeof h === "string" ? h : null;
}
