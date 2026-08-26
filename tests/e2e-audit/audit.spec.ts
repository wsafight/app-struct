import { expect, test } from "@playwright/test";

test("audit snapshots remain tenant isolated on desktop and mobile", async ({ page }, testInfo) => {
  const email = `audit-${Date.now()}@example.test`;
  const password = "AppStruct-Audit-E2E-2026";
  const projectName = `Audited project ${Date.now()}`;
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));

  await page.goto("/register");
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Create account" }).click();

  await page.getByLabel("Name").fill("Alpha UI");
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Project" })).toBeVisible();
  await page.getByRole("link", { name: "Add" }).click();
  await page.getByLabel("Name").fill(projectName);
  await page.getByLabel("Owner").selectOption({ label: email });
  await page.getByRole("button", { name: "Save" }).click();

  await page.getByRole("link", { name: "Audit log" }).click();
  await expect(page.getByRole("heading", { name: "Audit log" })).toBeVisible();
  await expect(page.getByText("1 events", { exact: true })).toBeVisible();
  await expect(page.getByText("create", { exact: true })).toBeVisible();
  await page.getByText("Change snapshot", { exact: true }).click();
  await expect(page.getByText(projectName, { exact: false })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("audit-desktop.png"), fullPage: true });

  page.once("dialog", (dialog) => dialog.accept("Beta UI"));
  await page.getByRole("button", { name: "Create organization" }).click();
  await expect(page.getByText("No audit events", { exact: true })).toBeVisible();
  await page.getByRole("combobox", { name: "Current organization" }).selectOption({ label: "Alpha UI" });
  await expect(page.getByText("1 events", { exact: true })).toBeVisible();

  await page.setViewportSize({ width: 390, height: 844 });
  const layout = await page.evaluate(() => {
    const frame = document.querySelector<HTMLElement>(".table-frame");
    return {
      contained: frame ? frame.getBoundingClientRect().right <= window.innerWidth : false,
      scrollable: frame ? frame.scrollWidth > frame.clientWidth : false,
    };
  });
  expect(layout).toEqual({ contained: true, scrollable: true });
  await page.screenshot({ path: testInfo.outputPath("audit-mobile.png"), fullPage: true });
  expect(errors).toEqual([]);
});
