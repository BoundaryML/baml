import { playwright } from "@vitest/browser-playwright";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "#bridge-web-core": fileURLToPath(new URL("./dist/wasm/bridge_web_core.js", import.meta.url)),
      "#bridge-web-native": fileURLToPath(new URL("./dist/native.js", import.meta.url)),
    },
  },
  test: {
    include: ["tests/*.test.ts"],
    browser: {
      enabled: true,
      headless: true,
      provider: playwright(),
      instances: [{ browser: "chromium" }],
    },
  },
});
