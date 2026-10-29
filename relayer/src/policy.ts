// Pure allowlist validation over a decoded delegate. No I/O.
export interface DecodedAction { type: string; methodName?: string; deposit: bigint; gas: bigint }
export interface DecodedDelegate {
  senderId: string; receiverId: string; maxBlockHeight: bigint; actions: DecodedAction[];
}
export interface PolicyConfig {
  coreContractId: string;
  allowedMethods?: string[];
  maxGasPerAction?: bigint;
  maxActions?: number;
  maxHeightAhead?: bigint;
}
export const ALLOWED_METHODS = ["register", "start_x_link", "verify_x_link"];
export const DEFAULT_MAX_GAS = 300_000_000_000_000n;

/** Returns null if the delegate is acceptable, else a rejection reason. */
export function checkPolicy(d: DecodedDelegate, cfg: PolicyConfig, currentHeight: bigint): string | null {
  const methods = cfg.allowedMethods ?? ALLOWED_METHODS;
  const maxGas = cfg.maxGasPerAction ?? DEFAULT_MAX_GAS;
  const maxActions = cfg.maxActions ?? 3;
  const ahead = cfg.maxHeightAhead ?? 1000n;
  if (d.receiverId !== cfg.coreContractId) return "receiver not allowed";
  if (d.actions.length === 0) return "no actions";
  if (d.actions.length > maxActions) return "too many actions";
  let total = 0n;
  for (const a of d.actions) {
    if (a.type !== "functionCall") return `action type not allowed: ${a.type}`;
    if (!a.methodName || !methods.includes(a.methodName)) return "method not allowed";
    if (a.deposit !== 0n) return "deposit must be zero";
    if (a.gas > maxGas) return "gas per action too high";
    total += a.gas;
  }
  if (total > maxGas) return "total gas too high";
  if (d.maxBlockHeight <= currentHeight) return "delegate expired";
  if (d.maxBlockHeight > currentHeight + ahead) return "max_block_height too far in the future";
  return null;
}
