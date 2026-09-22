import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";

export default defineConfig({
  optimizeDeps: {
    exclude: ["@boundaryml/baml-bridge-web"],
  },
  test: {
    env: { BAML_TEST_RUNTIME: "web" },
    include: ["web/**/*.test.ts"],
    // One headless chromium per pool worker; cap it for the same reason the
    // workers pool is capped.
    maxWorkers: 4,
    browser: {
      enabled: true,
      headless: true,
      provider: playwright(),
      instances: [{ browser: "chromium" }],
    },
  },
});
