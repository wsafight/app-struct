import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test.skip(
  process.env.APPSTRUCT_E2E_SAAS_PRODUCT !== "1",
  "Run against the generated saas-demo web application with APPSTRUCT_E2E_SAAS_PRODUCT=1",
);

async function productApi(page: Page, authenticated = false) {
  const state = {
    authenticated,
    organization: false,
    used: 0,
    paid: false,
    canceled: false,
    read: false,
    aiEnabled: false,
  };
  const user = {
    id: "user-product",
    email: "member@example.test",
    roles: ["member"],
  };
  const organization = {
    id: "org-product",
    name: "Product team",
    role: "owner",
  };
  const files: Record<string, unknown>[] = [];
  const runs: Record<string, unknown>[] = [];
  const calls: string[] = [];
  const profile = {
    display_name: "",
    locale: "en-US",
    timezone: "UTC",
    notifications: true,
    mfa_enabled: false,
  };
  const tasks = [
    {
      id: "task-product",
      title: "Review report",
      status: "todo",
      due_date: "2026-11-05",
      revision: 1,
    },
  ];
  const approvals: Record<string, unknown>[] = [];
  const ai: Record<string, unknown>[] = [];
  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const path = new URL(request.url()).pathname;
    const method = request.method();
    calls.push(`${method} ${path}`);
    const send = (body: unknown, status = 200) =>
      route.fulfill({ status, json: body });
    if (method === "OPTIONS") {
      return route.fulfill({
        status: 204,
        headers: {
          "Access-Control-Allow-Origin": new URL(page.url()).origin,
          "Access-Control-Allow-Credentials": "true",
          "Access-Control-Allow-Headers":
            "content-type,x-appstruct-tenant,idempotency-key,x-csrf-token",
          "Access-Control-Allow-Methods": "GET,POST,PUT,PATCH,DELETE,OPTIONS",
        },
      });
    }
    if (path === "/api/auth/me")
      return send({ user: state.authenticated ? user : null });
    if (path === "/api/auth/register" || path === "/api/auth/login") {
      state.authenticated = true;
      return send({ user });
    }
    if (path === "/api/auth/settings") {
      if (method === "PATCH") {
        Object.assign(profile, request.postDataJSON());
        return route.fulfill({ status: 204 });
      }
      return send(profile);
    }
    if (path === "/api/auth/settings/mfa/setup")
      return send({
        secret: "JBSWY3DPEHPK3PXP",
        uri: "otpauth://totp/product?secret=JBSWY3DPEHPK3PXP",
      });
    if (path === "/api/auth/settings/mfa/enable") {
      profile.mfa_enabled = true;
      return send({ recovery_codes: ["one-time-recovery-code"] });
    }
    if (path === "/api/product/notifications")
      return send({
        data: [
          {
            id: "notice-product",
            title: "Report ready",
            body: "Your result is available.",
            target: "/results",
            read_at: state.read ? new Date().toISOString() : null,
            created_at: new Date().toISOString(),
            tenant_id: null,
          },
        ],
        total: 1,
        unread: state.read ? 0 : 1,
      });
    if (
      path === "/api/product/notifications/read" ||
      path === "/api/product/notifications/notice-product/read"
    ) {
      state.read = true;
      return route.fulfill({ status: 204 });
    }
    if (path === "/api/product/team") {
      if (method === "PATCH") {
        organization.name = request.postDataJSON().name;
        return route.fulfill({ status: 204 });
      }
      return send({
        id: organization.id,
        name: organization.name,
        is_owner: true,
        seats_used: 2,
        seat_limit: 5,
        members_truncated: false,
        members: [
          { id: user.id, name: "You", role: "owner" },
          { id: "reviewer-product", name: "Reviewer", role: "member" },
        ],
      });
    }
    if (path === "/api/product/credits")
      return send({
        monthly_allowance: 20,
        monthly_remaining: 19,
        paid_balance: 100,
        available: 119,
        resets_at: "2026-12-01T00:00:00Z",
        data: [
          {
            id: "credit-entry",
            amount: -1,
            reason: "usage",
            created_at: new Date().toISOString(),
            expires_at: "2026-12-01T00:00:00Z",
          },
        ],
        page_size: 25,
      });
    if (path === "/api/tasks/" || path === "/api/projects/")
      return send({
        data: path === "/api/tasks/" ? tasks : [],
        meta: {
          page: 1,
          page_size: 100,
          total: path === "/api/tasks/" ? tasks.length : 0,
        },
      });
    if (path === "/api/product/approvals") {
      if (method === "POST") {
        const item = {
          id: "approval-product",
          ...request.postDataJSON(),
          requested_by: user.id,
          state: "pending",
          created_at: new Date().toISOString(),
          task_snapshot: tasks[0],
        };
        approvals.push(item);
        return send(item, 201);
      }
      return send({ data: approvals });
    }
    if (path === "/api/product/assistant/capabilities")
      return send({
        enabled: state.aiEnabled,
        provider: "ollama",
        credit_cost: 1,
        max_prompt_bytes: 8000,
        max_output_bytes: 65536,
        max_parallel_per_team: 2,
      });
    if (path === "/api/product/assistant") {
      if (method === "POST") {
        const item = {
          id: "ai-product",
          prompt: request.postDataJSON().prompt,
          model: "stub-model",
          state: "queued",
          answer: "",
          created_at: new Date().toISOString(),
        };
        ai.unshift(item);
        return send(item);
      }
      return send({ data: ai });
    }
    if (path === "/api/product/assistant/ai-product/events")
      return route.fulfill({
        contentType: "text/event-stream",
        body: `event: snapshot\ndata: ${JSON.stringify({ ...ai[0], state: "succeeded", answer: "A useful draft. <script>not executable</script>" })}\n\n`,
      });
    if (path === "/api/tenant/organizations") {
      if (method === "POST") {
        state.organization = true;
        return send(organization, 201);
      }
      return send({ data: state.organization ? [organization] : [] });
    }
    if (path === "/api/files") {
      if (method === "POST") {
        expect(request.headers()["x-appstruct-tenant"]).toBe(organization.id);
        const file = {
          id: "file-product",
          original_name: "input.txt",
          content_type: "text/plain",
          size: 24,
          created_at: new Date().toISOString(),
        };
        files.push(file);
        return send(file, 201);
      }
      return send({
        data: files,
        meta: { page: 1, page_size: 100, total: files.length },
      });
    }
    if (path === "/api/files/uploads")
      return send(
        {
          id: "upload-product",
          upload_url: "/api/files/uploads/upload-product/content",
          direct: false,
          expires_at: "2026-12-01T00:00:00Z",
        },
        201,
      );
    if (path === "/api/files/uploads/upload-product/content")
      return route.fulfill({ status: 204 });
    if (path === "/api/files/uploads/upload-product/complete") {
      const file = {
        id: "file-product",
        original_name: "input.txt",
        content_type: "text/plain",
        size: 24,
        created_at: new Date().toISOString(),
      };
      files.push(file);
      return send(file);
    }
    if (path === "/api/files/file-product/download") {
      return route.fulfill({
        contentType: "text/plain",
        body: "A useful team summary.",
      });
    }
    if (path === "/api/reports/templates/team-summary/runs") {
      expect(request.headers()["idempotency-key"]).toBeTruthy();
      expect(request.postDataJSON().data.summary).toBe(
        "A useful team summary.",
      );
      state.used += 1;
      const run = {
        id: `run-${state.used}`,
        template: "team-summary",
        stage: "succeeded",
        progress: 100,
        error_code: null,
        actor_id: user.id,
        created_at: new Date().toISOString(),
        expires_at: "2099-12-01T00:00:00Z",
      };
      runs.unshift(run);
      return send(run, 202);
    }
    if (path === "/api/reports/runs")
      return send({
        data: runs,
        meta: { page: 1, page_size: 10, total: runs.length },
      });
    if (/^\/api\/reports\/runs\/run-\d+\/download$/.test(path)) {
      return route.fulfill({
        contentType: "application/pdf",
        body: "%PDF-1.4\n%%EOF",
      });
    }
    if (path === "/api/billing")
      return send({
        plans: [{ id: "pro", trial_days: null, entitlements: ["reports"] }],
        subscription:
          state.paid || state.canceled
            ? {
                plan_id: "pro",
                status: state.canceled ? "canceled" : "active",
                cancel_at_period_end: false,
                current_period_end: null,
              }
            : null,
        entitlements: state.paid ? ["reports"] : [],
        customer_portal: true,
        checkout_available: true,
        usage: [
          {
            entitlement: "reports",
            free_limit: 3,
            used: state.used,
            remaining: Math.max(0, 3 - state.used),
            paid: state.paid,
          },
        ],
      });
    if (path === "/api/billing/checkout") {
      state.paid = true;
      return send({
        url: `${new URL(page.url()).origin}/billing?checkout=success`,
      });
    }
    return send(
      {
        error: {
          code: "TEST_UNEXPECTED_REQUEST",
          message: `${method} ${path}`,
          fields: [],
        },
      },
      500,
    );
  });
  return { state, calls, runs };
}

test("anonymous marketing pages do not enter tenant onboarding", async ({
  page,
}) => {
  const { calls } = await productApi(page);
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "Turn your team's data into a report worth sharing.",
  );
  await page.getByRole("link", { name: "Explore plans" }).click();
  await expect(
    page.getByRole("heading", { name: "Start small. Make room for more." }),
  ).toBeVisible();
  expect(calls.some((call) => call.includes("/api/tenant/"))).toBe(false);
});

test("account preferences and MFA recovery codes are usable without a team", async ({
  page,
}, testInfo) => {
  await productApi(page, true);
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async (value: string) => {
          (
            window as typeof window & { testCopiedText: string }
          ).testCopiedText = value;
        },
      },
    });
  });
  await page.goto("/account");
  await page.getByLabel("Display name").fill("Product member");
  await page.getByLabel("Report language").selectOption("zh-CN");
  await page.getByRole("button", { name: "Save preferences" }).click();
  await expect(page.getByText("Settings saved", { exact: true })).toBeVisible();
  await page
    .getByLabel("Current password", { exact: true })
    .fill("Product-Password-2026");
  await page
    .getByRole("button", { name: "Set up authenticator", exact: true })
    .click();
  await expect(
    page.getByRole("img", { name: "Authenticator setup QR code" }),
  ).toBeVisible();
  await page.screenshot({
    path: testInfo.outputPath("account-mfa.png"),
    fullPage: true,
  });
  expect(
    (await new AxeBuilder({ page }).include(".account-settings").analyze())
      .violations,
  ).toEqual([]);
  await page.getByText("Enter a setup key instead", { exact: true }).click();
  await expect(
    page.getByText("JBSWY3DPEHPK3PXP", { exact: true }),
  ).toBeVisible();
  await page.getByLabel("Authenticator or recovery code").fill("123456");
  await page.getByRole("button", { name: "Enable MFA", exact: true }).click();
  await expect(
    page.getByText("one-time-recovery-code", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "I saved my recovery codes" }),
  ).toBeDisabled();
  await page
    .getByRole("button", { name: "Copy recovery codes", exact: true })
    .click();
  await expect(
    page.getByText("Recovery codes copied", { exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        (window as typeof window & { testCopiedText: string }).testCopiedText,
    ),
  ).toBe("one-time-recovery-code");
  const download = page.waitForEvent("download");
  await page
    .getByRole("button", { name: "Download recovery codes", exact: true })
    .click();
  expect((await download).suggestedFilename()).toBe("mfa-recovery-codes.txt");
  await page.getByRole("link", { name: "Open workspace", exact: true }).click();
  const leaveDialog = page.getByRole("dialog", {
    name: "Leave without saving recovery codes?",
  });
  await expect(leaveDialog).toBeVisible();
  await leaveDialog
    .getByRole("button", { name: "Cancel", exact: true })
    .click();
  await page
    .getByLabel("I stored these codes in a safe place", { exact: true })
    .check();
  await page.getByRole("button", { name: "I saved my recovery codes" }).click();
  await expect(
    page.getByText("one-time-recovery-code", { exact: true }),
  ).toHaveCount(0);
});

test("notifications, team settings and monthly credit ledger", async ({
  page,
}) => {
  const { state } = await productApi(page, true);
  state.organization = true;
  await page.goto("/notifications");
  await expect(page.getByText("1 unread", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Mark all as read" }).click();
  await expect(page.getByText("0 unread", { exact: true })).toBeVisible();
  await page.goto("/team");
  await page.getByLabel("Team name", { exact: true }).fill("Renamed team");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Renamed team", exact: true }),
  ).toBeVisible();
  const creditLink = page.getByRole("link", { name: "Credits", exact: true });
  if (!(await creditLink.isVisible()))
    await page.getByRole("button", { name: "Toggle navigation" }).click();
  await creditLink.click();
  await expect(
    page.getByRole("heading", { name: "119 credits available" }),
  ).toBeVisible();
  await expect(
    page.getByRole("cell", { name: "usage", exact: true }),
  ).toBeVisible();
});

test("board and calendar share tasks, then request an approval", async ({
  page,
}) => {
  const { state, runs } = await productApi(page, true);
  state.organization = true;
  runs.push({
    id: "run-1",
    template: "team-summary",
    actor_id: "user-product",
    stage: "succeeded",
    expires_at: "2099-12-01T00:00:00Z",
  });
  await page.goto("/planning");
  await expect(
    page.getByRole("link", { name: "Review report", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Calendar", exact: true }).click();
  await page.getByLabel("Month", { exact: true }).fill("2026-11");
  await page.getByRole("button", { name: "Month grid", exact: true }).click();
  await expect(
    page
      .getByRole("region", { name: "2026-11-05", exact: true })
      .getByRole("link", { name: "Review report", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("link", { name: "Review approvals", exact: true })
    .click();
  await page
    .getByRole("combobox", { name: "Task", exact: true })
    .selectOption("task-product");
  await page
    .getByRole("combobox", { name: "Completed report", exact: true })
    .selectOption("run-1");
  await page
    .getByRole("combobox", { name: "Reviewer", exact: true })
    .selectOption("reviewer-product");
  await page
    .getByRole("button", { name: "Submit for approval", exact: true })
    .click();
  await expect(page.getByText(/^Pending review · Submitted /)).toBeVisible();
});

test("AI is optional and streamed output remains text", async ({ page }) => {
  const { state } = await productApi(page, true);
  state.organization = true;
  await page.goto("/assistant");
  await expect(page.getByText(/AI is disabled\. Configure/)).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Generate draft" }),
  ).toBeDisabled();
  state.aiEnabled = true;
  await page.reload();
  await page
    .getByLabel("What should the assistant draft?")
    .fill("Draft a summary");
  await page.getByRole("button", { name: "Generate draft" }).click();
  await expect(page.locator(".ai-answer")).toHaveText(
    "A useful draft. <script>not executable</script>",
  );
  await expect(page.locator(".ai-answer script")).toHaveCount(0);
});

test("account works before creating an organization", async ({ page }) => {
  await productApi(page, true);
  await page.goto("/account");
  await expect(
    page.getByRole("heading", { name: "Account", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("member@example.test", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Create organization" }),
  ).toHaveCount(0);
  await page.getByRole("link", { name: "Open workspace" }).click();
  await expect(
    page.getByRole("heading", { name: "Create organization" }),
  ).toBeVisible();
});

test("a canceled subscription can purchase the same plan again", async ({
  page,
}) => {
  const { state } = await productApi(page, true);
  state.organization = true;
  state.canceled = true;
  state.used = 3;
  await page.goto("/billing");
  await expect(
    page.getByRole("button", { name: "Choose plan", exact: true }),
  ).toBeEnabled();
});

test("destination survives login, registration, refresh and tenant onboarding", async ({
  page,
}) => {
  await productApi(page);
  const destination = "/studio?source=invite#input";
  await page.goto(destination);
  await expect(
    page.getByRole("heading", { name: "Sign in", exact: true }),
  ).toBeVisible();
  expect(new URL(page.url()).searchParams.get("next")).toBe(destination);
  await page.getByRole("link", { name: "Create account", exact: true }).click();
  await page.reload();
  expect(new URL(page.url()).searchParams.get("next")).toBe(destination);
  await page.getByLabel("Email", { exact: true }).fill("member@example.test");
  await page
    .getByLabel("Password", { exact: true })
    .fill("Product-Password-2026");
  await page
    .getByRole("button", { name: "Create account", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Create organization" }),
  ).toBeVisible();
  await page.getByLabel("Name", { exact: true }).fill("Product team");
  await page.getByRole("button", { name: "Create", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Your report studio" }),
  ).toBeVisible();
  await expect(page).toHaveURL(new RegExp("/studio\\?source=invite#input$"));
});

test("upload, generate, download, exhaust allowance and upgrade", async ({
  page,
}) => {
  const { state } = await productApi(page, true);
  state.organization = true;
  const pageErrors: string[] = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await page.goto("/studio");
  await page.getByLabel("File", { exact: true }).setInputFiles({
    name: "input.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("A useful team summary."),
  });
  await page.getByRole("button", { name: "Upload", exact: true }).click();
  await expect(page.getByRole("combobox", { name: "Input file" })).toHaveValue(
    "file-product",
  );
  for (const index of [1, 2, 3]) {
    await page.getByLabel("Report title").fill(`Report ${index}`);
    await page
      .getByRole("button", { name: "Generate report", exact: true })
      .click();
    await expect(page.locator(".studio-run")).toHaveCount(index);
  }
  await expect(
    page.getByRole("heading", { name: "Upgrade to continue" }),
  ).toBeVisible();
  const download = page.waitForEvent("download");
  await page
    .getByRole("button", { name: "Download PDF", exact: true })
    .first()
    .click();
  expect((await download).suggestedFilename()).toBe("team-report.pdf");
  await page.getByRole("link", { name: "View plans", exact: true }).click();
  await page.getByRole("button", { name: "Choose plan", exact: true }).click();
  await expect(page).toHaveURL(/\/billing\?checkout=success$/);
  await expect(
    page.getByText("Included in your plan", { exact: true }),
  ).toBeVisible();
  await page.goto("/studio");
  await expect(
    page.getByRole("button", { name: "Generate report", exact: true }),
  ).toBeVisible();
  expect(pageErrors).toEqual([]);
});

test("calendar stays readable on mobile and navigation closes after selection", async ({
  page,
}, testInfo) => {
  const { state } = await productApi(page, true);
  state.organization = true;
  await page.goto("/planning");
  await page.getByRole("button", { name: "Calendar", exact: true }).click();
  await page.getByLabel("Month", { exact: true }).fill("2026-11");
  if (testInfo.project.name === "chromium-mobile") {
    await expect(
      page.getByRole("button", { name: "Agenda", exact: true }),
    ).toHaveAttribute("aria-pressed", "true");
    await expect(
      page
        .locator(".planning-agenda")
        .getByRole("link", { name: "Review report" }),
    ).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath("planning-agenda.png"),
      fullPage: true,
    });
  }
  await page.getByRole("button", { name: "Month grid", exact: true }).click();
  const calendar = page.getByRole("region", {
    name: "Task calendar",
    exact: true,
  });
  await expect(
    calendar.getByRole("region", { name: "2026-11-05", exact: true }),
  ).toBeVisible();
  const bounds = await calendar.locator(".calendar-day").evaluateAll((days) =>
    days.slice(0, 7).map((day) => {
      const rect = day.getBoundingClientRect();
      return { left: rect.left, right: rect.right };
    }),
  );
  expect(bounds).toHaveLength(7);
  for (let index = 1; index < bounds.length; index += 1)
    expect(bounds[index].left).toBeGreaterThanOrEqual(bounds[index - 1].right);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  expect(
    (await new AxeBuilder({ page }).include(".planning-page").analyze())
      .violations,
  ).toEqual([]);
  await page.screenshot({
    path: testInfo.outputPath("planning-calendar.png"),
    fullPage: true,
  });
  if (testInfo.project.name === "chromium-mobile") {
    const toggle = page.getByRole("button", { name: "Toggle navigation" });
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-expanded", "true");
    await page
      .getByRole("link", { name: "Team settings", exact: true })
      .click();
    await expect(
      page.getByRole("heading", { name: "Team settings", exact: true }),
    ).toBeVisible();
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await page.screenshot({
      path: testInfo.outputPath("team-mobile.png"),
      fullPage: true,
    });
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= window.innerWidth,
      ),
    ).toBe(true);
    await toggle.click();
    await page.getByText("Advanced tools", { exact: true }).click();
    await expect(
      page.getByRole("link", { name: "API tokens", exact: true }),
    ).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await expect(toggle).toBeFocused();
  }
});

test("report polling stops once work reaches a terminal state", async ({
  page,
}) => {
  const { state, runs, calls } = await productApi(page, true);
  state.organization = true;
  const run = {
    id: "run-1",
    template: "team-summary",
    stage: "queued",
    progress: 0,
    error_code: null,
    actor_id: "user-product",
    created_at: "2026-11-06T00:00:00Z",
    expires_at: "2099-12-01T00:00:00Z",
  };
  runs.push(run);
  const reportCalls = () =>
    calls.filter((call) => call === "GET /api/reports/runs").length;
  await page.goto("/results");
  await expect(page.getByText("Queued", { exact: true })).toBeVisible();
  await expect.poll(reportCalls).toBeGreaterThanOrEqual(2);
  run.stage = "succeeded";
  run.progress = 100;
  await expect(page.getByText("Completed", { exact: true })).toBeVisible();
  const terminalCalls = reportCalls();
  await page.waitForTimeout(2500);
  expect(reportCalls()).toBe(terminalCalls);
});

test("notification row actions stay independent and share the badge cache", async ({
  page,
}) => {
  const { state } = await productApi(page, true);
  state.organization = true;
  await page.addInitScript(() =>
    window.localStorage.setItem("appstruct_tenant", "org-product"),
  );
  let releaseFirst: () => void = () => undefined;
  const firstRequest = new Promise<void>((resolve) => {
    releaseFirst = resolve;
  });
  const read = new Set<string>();
  let reads = 0;
  await page.route("**/api/product/notifications**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    if (route.request().method() === "GET") {
      reads += 1;
      return route.fulfill({
        json: {
          data: ["first", "second"].map((id) => ({
            id,
            title: `${id} notification`,
            body: "A team update",
            target: "/results",
            tenant_id: null,
            read_at: read.has(id) ? "2026-11-06T00:00:00Z" : null,
            created_at: "2026-11-06T00:00:00Z",
          })),
          total: 2,
          unread: 2 - read.size,
        },
      });
    }
    const id = path.split("/").at(-2)!;
    if (id === "first") await firstRequest;
    read.add(id);
    return route.fulfill({ status: 204 });
  });
  await page.goto("/planning");
  const toggle = page.getByRole("button", { name: "Toggle navigation" });
  if (page.viewportSize()!.width < 760) await toggle.click();
  await expect(
    page.getByLabel("2 unread notifications", { exact: true }),
  ).toBeVisible();
  const initialReads = reads;
  await page.getByRole("link", { name: /^Notifications/ }).click();
  const first = page
    .getByRole("article")
    .filter({ hasText: "first notification" });
  const second = page
    .getByRole("article")
    .filter({ hasText: "second notification" });
  const button = (card: typeof first) =>
    card.getByRole("button", { name: "Mark as read", exact: true });
  await expect(button(first)).toBeVisible();
  expect(reads).toBe(initialReads);
  try {
    await button(first).click();
    await expect(first).toHaveAttribute("aria-busy", "true");
    await expect(button(second)).toBeEnabled();
    await button(second).click();
    await expect(second.getByRole("button")).toHaveCount(0);
    await expect(first).toHaveAttribute("aria-busy", "true");
  } finally {
    releaseFirst();
  }
  await expect(page.getByText("0 unread", { exact: true })).toBeVisible();
  await expect(first.getByRole("button")).toHaveCount(0);
});

test("MFA clipboard failures provide a safe manual fallback", async ({
  page,
}) => {
  await productApi(page, true);
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: async () => {
          throw new Error("Permission denied");
        },
      },
    });
  });
  await page.goto("/account");
  await page
    .getByLabel("Current password", { exact: true })
    .fill("Product-Password-2026");
  await page
    .getByRole("button", { name: "Set up authenticator", exact: true })
    .click();
  await page.getByText("Enter a setup key instead", { exact: true }).click();
  await page
    .getByRole("button", { name: "Copy setup key", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText(
    "Clipboard unavailable. Select and copy the visible text instead.",
  );
  await expect(
    page.getByText("JBSWY3DPEHPK3PXP", { exact: true }),
  ).toBeVisible();
});

test("AI prompt limits count UTF-8 bytes rather than characters", async ({
  page,
}) => {
  const { state } = await productApi(page, true);
  state.organization = true;
  state.aiEnabled = true;
  await page.goto("/assistant");
  await page
    .getByLabel("What should the assistant draft?")
    .fill("中".repeat(2667));
  await expect(page.getByRole("alert")).toHaveText("8,001 / 8,000 UTF-8 bytes");
  await expect(
    page.getByRole("button", { name: "Generate draft" }),
  ).toBeDisabled();
});
