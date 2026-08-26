import { expect, test } from "@playwright/test";

test("tenant onboarding switching and responsive isolation UI", async ({ page }, testInfo) => {
  const email = `tenant-${Date.now()}@example.test`;
  const password = "AppStruct-Tenant-E2E-2026";
  const projectName = `Tenant project ${Date.now()}`;
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));

  await page.goto("/register");
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Create account" }).click();

  await expect(page.getByRole("heading", { name: "Create organization" })).toBeVisible();
  await page.getByLabel("Name").fill("Alpha");
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await expect(page.getByRole("combobox", { name: "Current organization" })).toHaveValue(/.+/);
  await expect(page.getByRole("heading", { name: "Project" })).toBeVisible();

  await page.getByRole("link", { name: "Add" }).click();
  await page.getByLabel("Name").fill(projectName);
  await page.getByLabel("Owner").selectOption({ label: email });
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText(projectName, { exact: true })).toBeVisible();

  page.once("dialog", (dialog) => dialog.accept("Beta"));
  await page.getByRole("button", { name: "Create organization" }).click();
  await expect(page.getByRole("combobox", { name: "Current organization" })).toHaveValue(/.+/);
  await expect(page.getByText("No records", { exact: true })).toBeVisible();

  await page.getByRole("combobox", { name: "Current organization" }).selectOption({ label: "Alpha" });
  await expect(page.getByText(projectName, { exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("tenant-desktop.png"), fullPage: true });

  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole("combobox", { name: "Current organization" })).toBeVisible();
  const layout = await page.evaluate(() => {
    const table = document.querySelector<HTMLElement>(".table-frame");
    return {
      tableContained: table ? table.getBoundingClientRect().right <= window.innerWidth : false,
      tableScrollable: table ? table.scrollWidth > table.clientWidth : false,
    };
  });
  expect(layout).toEqual({ tableContained: true, tableScrollable: true });
  await page.screenshot({ path: testInfo.outputPath("tenant-mobile.png"), fullPage: true });
  expect(errors).toEqual([]);
});
