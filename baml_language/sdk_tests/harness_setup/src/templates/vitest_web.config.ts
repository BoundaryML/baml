import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";

export default defineConfig({
  optimizeDeps: {
    exclude: ["@boundaryml/baml-bridge-web"],
  },
  test: {
    include: ["**/*.test.ts"],
    browser: {
      enabled: true,
      provider: playwright({ launchOptions: { channel: "chrome" } }),
      instances: [{ browser: "chromium" }],
      headless: true,
    },
  },
});
