import { describe, expect, it } from "vitest";
import { intentUrl, linkPostText, profileUrl } from "./x";
import { capStatus } from "./cap";
import { chartGeometry, niceMax } from "./chart";
import { resolveConfig } from "./config";
import { navItems, parseRoute } from "./route";
import type { Epoch, Settings, User } from "./types";

describe("X intent", () => {
  it("encodes the post text", () => {
    expect(linkPostText("GM1abc")).toBe("gm GM1abc");
    expect(intentUrl("GM1abc")).toBe("https://x.com/intent/post?text=gm%20GM1abc");
    expect(new URL(intentUrl("a&b")).searchParams.get("text")).toBe("gm a&b");
  });
  it("profile url strips @", () => {
    expect(profileUrl("@bob")).toBe("https://x.com/bob");
  });
});

const settings = { unverifiedCap: 100n } as Settings;
const user = (o: Partial<User>) => ({ verifiedHuman: false, mintedUnverified: 0n, ...o }) as User;

describe("cap status", () => {
  it("null without data", () => {
    expect(capStatus(null, settings)).toBeNull();
    expect(capStatus(user({}), null)).toBeNull();
  });
  it("partial, reached, verified", () => {
    expect(capStatus(user({ mintedUnverified: 25n }), settings)).toMatchObject({ capped: true, remaining: 75n, pct: 25, reached: false });
    expect(capStatus(user({ mintedUnverified: 100n }), settings)).toMatchObject({ remaining: 0n, pct: 100, reached: true });
    expect(capStatus(user({ verifiedHuman: true, mintedUnverified: 100n }), settings)).toMatchObject({ capped: false, reached: false });
  });
});

describe("chart", () => {
  const ep = (index: number, budget: bigint, minted: bigint) => ({ index, budget, minted }) as Epoch;
  const G = 10n ** 18n;
  it("niceMax", () => {
    expect(niceMax(0)).toBe(1);
    expect(niceMax(730)).toBe(1000);
    expect(niceMax(180)).toBe(200);
    expect(niceMax(1000)).toBe(1000);
  });
  it("bars and line fit inside the box", () => {
    const box = { width: 400, height: 200, left: 40, right: 10, top: 10, bottom: 20 };
    const g = chartGeometry([ep(0, 100n * G, 50n * G), ep(1, 200n * G, 0n)], box);
    expect(g.yMax).toBe(200);
    expect(g.bars).toHaveLength(2);
    expect(g.bars[1].y).toBeCloseTo(10);
    expect(g.bars[1].h).toBeCloseTo(170);
    expect(g.line[0].y).toBeCloseTo(10 + 170 * 0.75);
    expect(g.line[1].y).toBeCloseTo(180);
  });
  it("handles empty input", () => {
    const g = chartGeometry([], { width: 100, height: 100, left: 10, right: 10, top: 10, bottom: 10 });
    expect(g.bars).toEqual([]);
    expect(g.yMax).toBe(1);
  });
});

describe("config and routes", () => {
  it("defaults rpc per network and flags missing contracts", () => {
    const c = resolveConfig({ VITE_NETWORK: "testnet" });
    expect(c.rpc).toBe("https://rpc.testnet.fastnear.com");
    expect(c.errors.length).toBe(2);
    expect(resolveConfig({ VITE_NETWORK: "mainnet", VITE_CORE: "a", VITE_TOKEN: "b" }).rpc).toContain("mainnet");
  });
  it("reads staging, relayer and rpc override", () => {
    const c = resolveConfig({ VITE_CORE: "a", VITE_TOKEN: "b", VITE_STAGING: "1", VITE_RELAYER: "https://r", VITE_RPC: "https://x" });
    expect(c).toMatchObject({ staging: true, relayer: "https://r", rpc: "https://x", errors: [] });
    expect(resolveConfig({ VITE_CORE: "a", VITE_TOKEN: "b" }).staging).toBe(false);
  });
  it("routes, hiding /lab without staging", () => {
    expect(parseRoute("", false)).toBe("home");
    expect(parseRoute("#/budget", false)).toBe("budget");
    expect(parseRoute("#/lab", false)).toBe("home");
    expect(parseRoute("#/lab", true)).toBe("lab");
    expect(navItems(false).some((i) => i.route === "lab")).toBe(false);
    expect(navItems(true).some((i) => i.route === "lab")).toBe(true);
  });
});
