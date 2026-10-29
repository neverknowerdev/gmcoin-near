import { describe, expect, it } from "vitest";
import { advanceClock, bellCall, decodeSuccessValue, GAS_300, outcomeFailure, register, startXLink, verifyXLink } from "./calls";
import { decodeViewResponse } from "./rpc";

const CORE = "core.testnet";

describe("call builders", () => {
  it("register is sponsored with zero deposit, or pays the deposit", () => {
    expect(register(CORE, null)).toMatchObject({ method: "register", deposit: 0n, receiver: CORE });
    expect(register(CORE, 5n).deposit).toBe(5n);
  });
  it("start_x_link has no args", () => {
    expect(startXLink(CORE)).toMatchObject({ method: "start_x_link", args: {}, deposit: 0n });
  });
  it("verify_x_link uses 300 TGas and trims post_url", () => {
    const c = verifyXLink(CORE, "  https://x.com/a/status/1  ", null);
    expect(c.gas).toBe(GAS_300);
    expect(c.gas).toBe(300_000_000_000_000n);
    expect(c.args).toEqual({ post_url: "https://x.com/a/status/1" });
    expect(verifyXLink(CORE, "   ", null).args).toEqual({ post_url: null });
  });
  it("ticks use 300 TGas and no deposit", () => {
    for (const m of ["scan_tick", "settle_day", "close_epoch"] as const) {
      expect(bellCall(CORE, m)).toMatchObject({ method: m, gas: GAS_300, deposit: 0n, args: {} });
    }
  });
  it("advance_clock passes by_ns as a string", () => {
    expect(advanceClock(CORE, 600_000_000_000n).args).toEqual({ by_ns: "600000000000" });
  });
});

describe("outcomes", () => {
  it("decodes SuccessValue", () => {
    expect(decodeSuccessValue({ status: { SuccessValue: btoa("true") } })).toBe(true);
    expect(decodeSuccessValue({ status: { SuccessValue: btoa("false") } })).toBe(false);
    expect(decodeSuccessValue({ status: { SuccessValue: "" } })).toBeUndefined();
    expect(decodeSuccessValue(null)).toBeUndefined();
  });
  it("extracts failures", () => {
    expect(outcomeFailure({ status: { SuccessValue: "" } })).toBeNull();
    expect(
      outcomeFailure({ status: { Failure: { ActionError: { kind: { FunctionCallError: { ExecutionError: "boom" } } } } } }),
    ).toBe("boom");
  });
});

describe("decodeViewResponse", () => {
  const bytes = (s: string) => Array.from(new TextEncoder().encode(s));
  it("decodes JSON bytes", () => {
    expect(decodeViewResponse({ result: { result: bytes('{"a":"1"}') } })).toEqual({ a: "1" });
  });
  it("returns null for the literal null and empty result", () => {
    expect(decodeViewResponse({ result: { result: bytes("null") } })).toBeNull();
    expect(decodeViewResponse({ result: { result: [] } })).toBeNull();
  });
  it("throws readable errors", () => {
    expect(() => decodeViewResponse({ error: { message: "bad" } }, "status")).toThrow(/status: bad/);
    expect(() => decodeViewResponse({ result: { error: "no such method" } }, "x")).toThrow(/no such method/);
    expect(() => decodeViewResponse({}, "x")).toThrow(/empty/);
  });
});
