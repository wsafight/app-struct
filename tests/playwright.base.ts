import { defineConfig, devices, type PlaywrightTestConfig } from "@playwright/test";

const desktop = {
  name: "chromium-desktop",
  use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 900 } },
};

/**
 * Shared Playwright config for the e2e suites. Each root `playwright.*.config.ts`
 * calls this with its test directory and the default dev-server port; the main
 * suite additionally runs a mobile-viewport project.
 */
export function makeConfig(
  testDir: string,
  defaultPort: number,
  extraProjects: PlaywrightTestConfig["projects"] = [],
): PlaywrightTestConfig {
  return defineConfig({
    testDir,
    fullyParallel: false,
    forbidOnly: true,
    retries: 0,
    reporter: "line",
    timeout: 30_000,
    use: {
      baseURL: process.env.PLAYWRIGHT_BASE_URL ?? `http://127.0.0.1:${defaultPort}`,
      screenshot: "only-on-failure",
      trace: "retain-on-failure",
    },
    projects: [desktop, ...extraProjects],
  });
}
