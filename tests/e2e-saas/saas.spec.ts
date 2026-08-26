import { expect, test } from "@playwright/test";

test("SaaS template supports tenant work and audit on desktop and mobile", async ({ page }, testInfo) => {
  const email = process.env.SAAS_E2E_EMAIL ?? "saas-admin@example.test";
  const password = process.env.SAAS_E2E_PASSWORD ?? "AppStruct-SaaS-E2E-2026";
  const projectName = `SaaS project ${Date.now()}`;
  const taskTitle = `SaaS task ${Date.now()}`;
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));

  await page.goto("/login");
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();

  await expect(page.getByRole("link", { name: "Users" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Audit log" })).toBeVisible();
  await page.getByRole("link", { name: "Projects" }).click();
  await expect(page.getByRole("heading", { name: "Projects" })).toBeVisible();
  await page.getByRole("link", { name: "Add" }).click();
  await page.getByLabel("Name").fill(projectName);
  await page.getByLabel("Description").fill("Created from the locked SaaS preset.");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText(projectName, { exact: true })).toBeVisible();

  await page.getByRole("link", { name: "Tasks" }).click();
  await page.getByRole("link", { name: "Add" }).click();
  await page.getByLabel("Title").fill(taskTitle);
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText(taskTitle, { exact: true })).toBeVisible();

  await page.getByRole("link", { name: "Audit log" }).click();
  await expect(page.getByRole("heading", { name: "Audit log" })).toBeVisible();
  await expect(page.getByText("2 events", { exact: true })).toBeVisible();
  await expect(page.getByText("app::Project", { exact: true })).toBeVisible();
  await expect(page.getByText("app::Task", { exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("saas-desktop.png"), fullPage: true });

  page.once("dialog", (dialog) => dialog.accept("Beta workspace"));
  await page.getByRole("button", { name: "Create organization" }).click();
  await expect(page.getByText("No audit events", { exact: true })).toBeVisible();
  await page.getByRole("link", { name: "Projects" }).click();
  await expect(page.getByText("No records", { exact: true })).toBeVisible();

  await page.getByRole("combobox", { name: "Current organization" }).selectOption({ label: "Alpha workspace" });
  await expect(page.getByText(projectName, { exact: true })).toBeVisible();

  await page.setViewportSize({ width: 390, height: 844 });
  const layout = await page.evaluate(() => {
    const frame = document.querySelector<HTMLElement>(".table-frame");
    return {
      tableContained: frame ? frame.getBoundingClientRect().right <= window.innerWidth : false,
      tableScrollable: frame ? frame.scrollWidth > frame.clientWidth : false,
    };
  });
  expect(layout).toEqual({ tableContained: true, tableScrollable: true });
  await page.screenshot({ path: testInfo.outputPath("saas-mobile.png"), fullPage: true });
  expect(errors).toEqual([]);
});

test("member navigation only exposes authorized resources", async ({ page }) => {
  const email = `saas-member-${Date.now()}@example.test`;
  const password = "AppStruct-Member-E2E-2026";

  await page.goto("/register");
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Create account" }).click();

  await expect(page.getByRole("heading", { name: "Create organization" })).toBeVisible();
  await page.getByLabel("Name").fill("Member workspace");
  await page.getByRole("button", { name: "Create" }).click();

  await expect(page.getByRole("link", { name: "Projects" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Tasks" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Users" })).toHaveCount(0);
  await expect(page.getByRole("link", { name: "Audit log" })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Projects" })).toBeVisible();

  await page.goto("/users");
  await expect(page.getByRole("alert")).toContainText("do not have permission");
  await page.goto("/audit");
  await expect(page.getByRole("alert")).toContainText("do not have permission");
});
