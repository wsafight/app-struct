import { expect, test } from "@playwright/test";

const email = "operator@operations.example.test";
const password = "AppStruct-Operations-E2E-2026";

test("combined operations remain usable on desktop and mobile", async ({ page }, testInfo) => {
  const orderId = requiredEnvironment("OPERATIONS_E2E_ORDER_ID");
  const orderLineId = requiredEnvironment("OPERATIONS_E2E_ORDER_LINE_ID");
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));

  await page.goto("/login");
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();

  const tenant = page.getByRole("combobox", { name: "Current organization" });
  await tenant.selectOption({ label: "Operations Alpha" });

  await page.goto("/order_lines");
  await expect(page.getByRole("columnheader", { name: "Unit price" })).toBeVisible();
  await expect(page.getByRole("columnheader", { name: "Currency" })).toHaveCount(0);
  await expect(page.getByText(/CNY\s+19\.95/)).toBeVisible();
  await page.goto(`/order_lines/${orderLineId}/edit`);
  await expect(page.getByRole("alert")).toContainText("permission");

  await page.goto(`/orders/${orderId}`);
  await expect(page.getByRole("heading", { name: "OPS-2026-0001" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Edit lines" })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Activity" })).toBeVisible();
  await expect(page.getByText("Approved for fulfillment", { exact: true })).toBeVisible();
  await expect(page.getByText("workflow · submit", { exact: true })).toBeVisible();
  await expect(page.getByText("workflow · approve", { exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("operations-detail-desktop.png"), fullPage: true });

  await page.getByRole("link", { name: "Reports" }).click();
  await expect(page.getByRole("heading", { name: "Reports" })).toBeVisible();
  await expect(page.getByText("succeeded", { exact: true })).toBeVisible();
  await expect(page.getByText("cancelled", { exact: true })).toBeVisible();

  await page.setViewportSize({ width: 390, height: 844 });
  const layout = await page.evaluate(() => {
    const frame = document.querySelector<HTMLElement>(".table-frame");
    return {
      contained: frame ? frame.getBoundingClientRect().right <= window.innerWidth : false,
      scrollable: frame ? frame.scrollWidth > frame.clientWidth : false,
    };
  });
  expect(layout).toEqual({ contained: true, scrollable: true });
  await page.screenshot({ path: testInfo.outputPath("operations-reports-mobile.png"), fullPage: true });

  const draft = await page.evaluate(async ({ owner, api }) => {
    const tenant = document.querySelector<HTMLSelectElement>('select[aria-label="Current organization"]')!.value;
    const csrf = document.cookie.split("; ").find((cookie) => cookie.startsWith("appstruct_csrf="))!.split("=")[1];
    const response = await fetch(`${api}/api/orders/`, { method: "POST", credentials: "include", headers: { "Content-Type": "application/json", "X-AppStruct-Tenant": tenant, "X-CSRF-Token": csrf }, body: JSON.stringify({ number: "OPS-EDITOR", owner_id: owner }) });
    if (!response.ok) throw new Error(`${response.status}: ${await response.text()}`);
    return { ...(await response.json()), tenant, csrf, api };
  }, { owner: requiredEnvironment("OPERATIONS_E2E_OPERATOR_ID"), api: requiredEnvironment("OPERATIONS_E2E_API") });
  await page.goto(`/orders/${draft.id}`);
  await page.getByRole("button", { name: "Edit lines" }).click();
  async function fillLine() {
    await page.getByRole("button", { name: "Add line" }).click();
    await page.getByRole("combobox", { name: "Product", exact: true }).selectOption(requiredEnvironment("OPERATIONS_E2E_PRODUCT_ID"));
    await page.getByRole("spinbutton", { name: "Quantity", exact: true }).fill("2");
    await page.getByLabel("Unit price currency").selectOption("CNY");
    await page.getByRole("spinbutton", { name: "Unit price", exact: true }).fill("19.95");
  }
  await fillLine();
  await page.evaluate(async ({ id, tenant, csrf, revision, api }) => {
    const response = await fetch(`${api}/api/orders/${id}`, { method: "PATCH", credentials: "include", headers: { "Content-Type": "application/json", "X-AppStruct-Tenant": tenant, "X-CSRF-Token": csrf, "If-Match": `"rev-${revision}"` }, body: JSON.stringify({ notes: "Concurrent edit" }) });
    if (!response.ok) throw new Error(await response.text());
  }, draft);
  await page.getByRole("button", { name: "Save lines" }).click();
  await expect(page.locator(".aggregate-section [role=alert]")).toBeVisible();
  await expect(page.getByRole("spinbutton", { name: "Quantity", exact: true })).toHaveValue("2");
  await page.getByRole("button", { name: "Reload", exact: true }).click();
  await expect(page.getByRole("spinbutton", { name: "Quantity", exact: true })).toHaveCount(0);
  await fillLine();
  await page.screenshot({ path: testInfo.outputPath("operations-aggregate-mobile.png"), fullPage: true });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.getByRole("button", { name: "Save lines" }).click();
  await expect(page.getByRole("button", { name: "Edit lines" })).toBeVisible();
  await expect(page.locator(".aggregate-section").getByText(/CNY\s+19\.95/)).toBeVisible();
  await page.getByRole("button", { name: "Edit lines" }).click();
  await page.getByRole("button", { name: "Remove line 1" }).click();
  await page.getByRole("button", { name: "Save lines" }).click();
  await expect(page.locator(".aggregate-section").getByText("No lines", { exact: true })).toBeVisible();
  expect(errors).toEqual([]);
});

test("supplier cannot open an operator order", async ({ page }) => {
  const orderId = requiredEnvironment("OPERATIONS_E2E_ORDER_ID");
  await page.goto("/login");
  await page.getByLabel("Email").fill("supplier@operations.example.test");
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await page
    .getByRole("combobox", { name: "Current organization" })
    .selectOption({ label: "Operations Alpha" });
  await page.goto(`/orders/${orderId}`);
  await expect(page.getByRole("alert")).toContainText("was not found");
});

function requiredEnvironment(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required`);
  return value;
}
