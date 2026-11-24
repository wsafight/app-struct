import { defineConfig, devices, type PlaywrightTestConfig } from "@playwright/test";

const desktop = {
  name: "chromium-desktop",
  use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 900 } },
};

const mobile: NonNullable<PlaywrightTestConfig["projects"]>[number] = {
  name: "chromium-mobile",
  testMatch: "saas-product.spec.ts",
  use: {
    ...devices["Desktop Chrome"],
    viewport: { width: 390, height: 844 },
    isMobile: true,
    hasTouch: true,
  },
};

const suites = {
  default: { testDir: "./tests/e2e", defaultPort: 55200, extraProjects: [mobile] },
  tenant: { testDir: "./tests/e2e-tenant", defaultPort: 55300, extraProjects: [] },
  audit: { testDir: "./tests/e2e-audit", defaultPort: 55400, extraProjects: [] },
  saas: { testDir: "./tests/e2e-saas", defaultPort: 55700, extraProjects: [] },
  operations: { testDir: "./tests/e2e-operations", defaultPort: 55900, extraProjects: [] },
} as const;

type SuiteName = keyof typeof suites;
const requested = (process.env.PLAYWRIGHT_SUITE ?? "default") as SuiteName;
const suite = suites[requested];
if (!suite) {
  throw new Error(
    `Unknown PLAYWRIGHT_SUITE="${requested}". Expected one of: ${Object.keys(suites).join(", ")}`,
  );
}

export default defineConfig({
  testDir: suite.testDir,
  fullyParallel: false,
  forbidOnly: true,
  retries: 0,
  reporter: "line",
  timeout: 30_000,
  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL ?? `http://127.0.0.1:${suite.defaultPort}`,
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  projects: [desktop, ...suite.extraProjects],
});
