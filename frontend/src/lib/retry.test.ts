import { describe, expect, it } from "vitest";
import { isRateLimit, retryRateLimit } from "./retry";
import { parseTestCreds } from "./testCreds";

describe("retryRateLimit", () => {
  it("detects rate limit errors", () => {
    expect(isRateLimit(new Error("Rate limit exceeded"))).toBe(true);
    expect(isRateLimit(new Error("RPC HTTP 429"))).toBe(true);
    expect(isRateLimit(new Error("smart contract panicked"))).toBe(false);
  });
  it("retries with exponential backoff then succeeds", async () => {
    const waits: number[] = [];
    let n = 0;
    const v = await retryRateLimit(
      async () => {
        if (++n < 3) throw new Error("rate limit");
        return "ok";
      },
      { baseMs: 100, sleep: async (ms) => void waits.push(ms) },
    );
    expect(v).toBe("ok");
    expect(waits).toEqual([100, 200]);
  });
  it("does not retry other errors and gives up after `tries`", async () => {
    let n = 0;
    await expect(retryRateLimit(async () => { n++; throw new Error("boom"); }, { sleep: async () => {} })).rejects.toThrow("boom");
    expect(n).toBe(1);
    n = 0;
    await expect(retryRateLimit(async () => { n++; throw new Error("429"); }, { tries: 3, sleep: async () => {} })).rejects.toThrow("429");
    expect(n).toBe(3);
  });
});

describe("parseTestCreds", () => {
  const key = "ed25519:" + "1".repeat(88);
  it("accepts valid creds", () => {
    expect(parseTestCreds(" a.testnet ", key)).toEqual({ accountId: "a.testnet", privateKey: key });
  });
  it("rejects missing or malformed values", () => {
    expect(() => parseTestCreds(null, key)).toThrow(/localStorage/);
    expect(() => parseTestCreds("a.testnet", null)).toThrow(/localStorage/);
    expect(() => parseTestCreds("A B", key)).toThrow(/Bad test account/);
    expect(() => parseTestCreds("a.testnet", "secp256k1:abc")).toThrow(/ed25519/);
  });
});
