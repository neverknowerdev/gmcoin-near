import { describe, expect, it } from "vitest";
import { compact, formatGm, formatNear, formatUnits, percent, shortAccount, toBig } from "./format";

describe("formatUnits", () => {
  it("formats whole and fractional GM", () => {
    expect(formatGm("1000000000000000000")).toBe("1");
    expect(formatGm("1500000000000000000")).toBe("1.5");
    expect(formatGm("0")).toBe("0");
    expect(formatGm(1234567n * 10n ** 18n)).toBe("1,234,567");
  });
  it("truncates instead of rounding", () => {
    expect(formatGm("1999999999999999999")).toBe("1.9999");
    expect(formatGm("1999999999999999999", 2)).toBe("1.99");
  });
  it("marks dust below the precision", () => {
    expect(formatGm("1")).toBe("<0.0001");
    expect(formatGm("1", 2)).toBe("<0.01");
  });
  it("handles junk and null", () => {
    expect(formatGm(null)).toBe("-");
    expect(formatGm(undefined)).toBe("-");
    expect(formatGm("abc")).toBe("-");
  });
  it("uses 24 decimals for NEAR", () => {
    expect(formatNear("100000000000000000000000")).toBe("0.1");
    expect(formatUnits(5n * 10n ** 24n, 24)).toBe("5");
  });
});

describe("misc format", () => {
  it("toBig", () => {
    expect(toBig("12")).toBe(12n);
    expect(toBig("-1")).toBeNull();
    expect(toBig(null)).toBeNull();
    expect(toBig(3)).toBe(3n);
  });
  it("compact", () => {
    expect(compact(950)).toBe("950");
    expect(compact(1500)).toBe("1.5k");
    expect(compact(2_500_000)).toBe("2.5M");
  });
  it("percent clamps", () => {
    expect(percent(5n, 10n)).toBe(50);
    expect(percent(20n, 10n)).toBe(100);
    expect(percent(1n, 0n)).toBe(0);
  });
  it("shortAccount", () => {
    expect(shortAccount("alice.near")).toBe("alice.near");
    expect(shortAccount("a".repeat(64), 11)).toBe("aaaaa…aaaaa");
  });
});
