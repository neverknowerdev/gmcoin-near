import { expect, test } from "@playwright/test";

// Read-only checks against the live testnet staging deploy (no wallet).

test("Budget page shows live status and epoch history", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto("/#/budget");
  await expect(page.getByRole("heading", { name: "Status" })).toBeVisible();
  await expect(page.getByTestId("status")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByTestId("status-epoch")).toContainText(/\d+/);
  await expect(page.getByTestId("budget-chart")).toBeVisible({ timeout: 30_000 });
  expect(await page.getByTestId("budget-bar").count()).toBeGreaterThan(0);
  expect(await page.getByTestId("epoch-table").locator("tbody tr").count()).toBeGreaterThan(0);
  await expect(page.locator(".alert.error")).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("Leaderboard page renders without errors and offers an epoch picker", async ({ page }) => {
  await page.goto("/#/leaderboard");
  const picker = page.getByLabel("Epoch");
  await expect(picker).toBeVisible({ timeout: 30_000 });
  expect(await picker.locator("option").count()).toBeGreaterThan(0);
  // Either rows or the explicit empty state, never an error.
  await expect(page.getByTestId("lb-table").or(page.getByTestId("lb-empty"))).toBeVisible({ timeout: 30_000 });
  await expect(page.locator(".alert.error")).toHaveCount(0);
});

test("Lab tab is present in the staging build", async ({ page }) => {
  await page.goto("/#/lab");
  await expect(page.getByRole("heading", { name: /Lab/ })).toBeVisible();
  await expect(page.getByRole("link", { name: "Lab" })).toBeVisible();
});
