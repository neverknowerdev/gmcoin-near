import { describe, expect, it } from "vitest";
import { checkPolicy, type DecodedDelegate } from "../src/policy";

const cfg = { coreContractId: "core.gmcoin.testnet" };
const ok = (): DecodedDelegate => ({
  senderId: "a.testnet", receiverId: "core.gmcoin.testnet", maxBlockHeight: 1100n,
  actions: [{ type: "functionCall", methodName: "register", deposit: 0n, gas: 30_000_000_000_000n }],
});
const H = 1000n;

describe("policy", () => {
  it("accepts happy path", () => expect(checkPolicy(ok(), cfg, H)).toBeNull());
  it("rejects wrong receiver", () => { const d = ok(); d.receiverId = "evil.testnet"; expect(checkPolicy(d, cfg, H)).toMatch(/receiver/); });
  it("rejects non-FunctionCall", () => {
    for (const t of ["transfer", "addKey", "deployContract", "deleteAccount"]) {
      const d = ok(); d.actions.push({ type: t, deposit: 0n, gas: 0n }); expect(checkPolicy(d, cfg, H)).toMatch(/type/);
    }
  });
  it("rejects disallowed method", () => { const d = ok(); d.actions[0].methodName = "ft_transfer"; expect(checkPolicy(d, cfg, H)).toMatch(/method/); });
  it("rejects non-zero deposit", () => { const d = ok(); d.actions[0].deposit = 1n; expect(checkPolicy(d, cfg, H)).toMatch(/deposit/); });
  it("rejects excess gas per action", () => { const d = ok(); d.actions[0].gas = 300_000_000_000_001n; expect(checkPolicy(d, cfg, H)).toMatch(/gas/); });
  it("rejects excess total gas", () => {
    const d = ok(); d.actions = ["register", "start_x_link"].map((m) => ({ type: "functionCall", methodName: m, deposit: 0n, gas: 200_000_000_000_000n }));
    expect(checkPolicy(d, cfg, H)).toMatch(/total gas/);
  });
  it("rejects >3 actions and zero actions", () => {
    const d = ok(); d.actions = Array(4).fill(d.actions[0]); expect(checkPolicy(d, cfg, H)).toMatch(/too many/);
    d.actions = []; expect(checkPolicy(d, cfg, H)).toMatch(/no actions/);
  });
  it("rejects expired and too-far heights", () => {
    const d = ok(); d.maxBlockHeight = 1000n; expect(checkPolicy(d, cfg, H)).toMatch(/expired/);
    d.maxBlockHeight = 2001n; expect(checkPolicy(d, cfg, H)).toMatch(/far/);
    d.maxBlockHeight = 2000n; expect(checkPolicy(d, cfg, H)).toBeNull();
  });
});
