import { actions } from "near-api-js";
import { describe, expect, it } from "vitest";
import { decodeSignedDelegate } from "../src/decode";
import { checkPolicy } from "../src/policy";
import { fc, makeDelegate } from "./helpers";

const cfg = { coreContractId: "core.gmcoin.testnet" };
describe("decode round-trip", () => {
  it("decodes a real signed delegate and policy accepts it", async () => {
    const { delegate } = decodeSignedDelegate(await makeDelegate([fc("register"), fc("start_x_link")]));
    expect(delegate.senderId).toBe("alice.testnet");
    expect(delegate.actions.map((a) => a.methodName)).toEqual(["register", "start_x_link"]);
    expect(delegate.actions[0]).toMatchObject({ type: "functionCall", deposit: 0n, gas: 30_000_000_000_000n });
    expect(checkPolicy(delegate, cfg, 1000n)).toBeNull();
  });
  it("rejects non-zero deposit", async () => {
    const { delegate } = decodeSignedDelegate(await makeDelegate([fc("register", 1n)]));
    expect(checkPolicy(delegate, cfg, 1000n)).toMatch(/deposit/);
  });
  it("rejects Transfer", async () => {
    const { delegate } = decodeSignedDelegate(await makeDelegate([actions.transfer(1n)]));
    expect(checkPolicy(delegate, cfg, 1000n)).toMatch(/type/);
  });
  it("throws on garbage", () => expect(() => decodeSignedDelegate("AAAA")).toThrow());
});
