import { expect, test } from "@playwright/test";

test("API health endpoints propagate request IDs", async ({ request }) => {
  const apiUrl = process.env.PLAYWRIGHT_API_URL ?? "http://127.0.0.1:33200";
  const live = await request.get(`${apiUrl}/health/live`);
  expect(live.status()).toBe(204);
  expect(live.headers()["x-request-id"]).toBeTruthy();

  const ready = await request.get(`${apiUrl}/health/ready`, {
    headers: { "X-Request-Id": "m5-browser-health-check" },
  });
  expect(ready.status()).toBe(204);
  expect(ready.headers()["x-request-id"]).toBe("m5-browser-health-check");
});

test("dashboard registration and project lifecycle", async ({ page }, testInfo) => {
  const email = `m5-${Date.now()}@example.test`;
  const password = "AppStruct-E2E-Password-2026";
  const projectName = `Browser project ${Date.now()}`;
  const renamedProject = `${projectName} updated`;
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));

  await page.goto("/register");
  await expect(page.getByRole("heading", { name: "Create account" })).toBeVisible();
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Create account" }).click();
  await expect(page.getByText(email, { exact: true })).toBeVisible();

  await page.getByRole("link", { name: "Projects" }).click();
  await expect(page.getByRole("heading", { name: "Projects" })).toBeVisible();
  await page.getByRole("link", { name: "Add" }).click();
  await page.getByLabel("Name").fill(projectName);
  await page.getByLabel("Owner").selectOption({ label: email });
  await page.getByRole("button", { name: "Save" }).click();

  const row = page.getByRole("row").filter({ hasText: projectName });
  await expect(row).toBeVisible();
  await row.getByRole("link", { name: "View" }).click();
  await expect(page.getByText(projectName, { exact: true })).toBeVisible();
  await page.getByRole("link", { name: "Edit" }).click();
  await page.getByLabel("Name").fill(renamedProject);
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.getByText(renamedProject, { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Sign out" }).click();
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
  await page.getByLabel("Email").fill(email);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await page.getByRole("link", { name: "Projects" }).click();
  await expect(page.getByText(renamedProject, { exact: true })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("dashboard.png"), fullPage: true });
  expect(pageErrors).toEqual([]);
});

test("authentication screen fits a mobile viewport", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/login");
  await expect(page.getByRole("heading", { name: "Sign in" })).toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
  expect(overflow).toBe(false);
  await page.screenshot({ path: testInfo.outputPath("mobile-login.png"), fullPage: true });
});
