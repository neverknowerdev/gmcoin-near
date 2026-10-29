import { expect, test, type Page } from "@playwright/test";
import { bellState } from "../src/lib/bell";
import { call, coreStatus, createFundedSubaccount, loadAccount, MOCK_OUTLAYER } from "./chain";

const MASTER = process.env.GM_E2E_MASTER;
const POLL = [2000]; // never poll faster than every 2s (public RPC rate limits)

test.skip(!MASTER, "set GM_E2E_MASTER=<master account> to run the real-testnet wallet flow");
test.describe.configure({ mode: "serial" });

async function connectTestWallet(page: Page) {
  await page.getByRole("button", { name: "Connect wallet" }).first().click();
  await page.getByText("Test key wallet").click();
}

test("register, link X (scripted OutLayer) and ring the bell with a real testnet account", async ({ page }) => {
  test.setTimeout(8 * 60_000);
  const master = loadAccount(MASTER!);

  // Fresh subaccount. 0.3 NEAR is not enough: attaching 300 TGas reserves 0.3 NEAR up front (unused gas is refunded),
  // so the account needs more than that. Default 0.6 NEAR; override with GM_E2E_FUND_NEAR.
  const prefix = `e2e${Date.now().toString(36)}`;
  const sub = await createFundedSubaccount(master, prefix, BigInt(Math.round(Number(process.env.GM_E2E_FUND_NEAR ?? "0.6") * 1000)) * 10n ** 21n);

  await page.addInitScript(
    ([id, key]) => {
      localStorage.setItem("gm_test_account", id);
      localStorage.setItem("gm_test_key", key);
    },
    [sub.id, sub.privateKey],
  );

  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));

  await page.goto("/#/");
  await connectTestWallet(page);
  await expect(page.getByText(`Connected as`)).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText(sub.id).first()).toBeVisible();

  // Register (sponsored, 0 deposit).
  await page.getByRole("button", { name: "Register", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Your GM" })).toBeVisible({ timeout: 90_000 });
  await expect(page.getByTestId("balance")).toBeVisible();

  // Start the X link and read the code.
  await page.getByRole("button", { name: "Get link code" }).click();
  const codeEl = page.getByTestId("link-code");
  await expect(codeEl).toBeVisible({ timeout: 90_000 });
  const code = (await codeEl.innerText()).trim();
  expect(code).toMatch(/^GM/);
  const intent = await page.getByRole("link", { name: "Post on X" }).getAttribute("href");
  expect(intent).toContain("https://x.com/intent/post?text=");
  expect(decodeURIComponent(intent!)).toContain(code);

  // Script the mock OutLayer so x-verify succeeds with a unique X id.
  const xId = `${Date.now()}${Math.floor(Math.random() * 1000)}`;
  await call(master, MOCK_OUTLAYER, "clear", {});
  await call(master, MOCK_OUTLAYER, "push", {
    items: [{ Output: JSON.stringify({ ok: true, x_id: xId, handle: "e2e", post_id: "1" }) }],
  });

  await page.getByRole("button", { name: /^Verify/ }).click();
  await expect(page.getByTestId("x-linked")).toContainText("@e2e", { timeout: 120_000 });

  // Bell page: buttons mirror status.
  await page.evaluate(() => (location.hash = "#/bell"));
  await expect(page.getByRole("heading", { name: "Ring the GM bell" })).toBeVisible();
  await expect(page.getByTestId("status")).toBeVisible({ timeout: 30_000 });
  const status = await coreStatus();
  expect(status).not.toBeNull();
  const expected = bellState(status);
  for (const m of ["scan_tick", "settle_day", "close_epoch"] as const) {
    const btn = page.getByRole("button", { name: m, exact: true });
    await expect(btn).toBeVisible();
    // status can move between the page load and our read; poll until they agree.
    await expect.poll(async () => btn.isEnabled(), { intervals: POLL, timeout: 20_000 }).toBe(expected[m]);
  }
  expect(errors).toEqual([]);
});
