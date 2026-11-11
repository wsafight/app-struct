import { devices } from "@playwright/test";
import { makeConfig } from "./tests/playwright.base";

export default makeConfig("./tests/e2e", 55200, [
  {
    name: "chromium-mobile",
    testMatch: "saas-product.spec.ts",
    use: {
      ...devices["Desktop Chrome"],
      viewport: { width: 390, height: 844 },
      isMobile: true,
      hasTouch: true,
    },
  },
]);
