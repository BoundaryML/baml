import { cloudflareTest } from "@cloudflare/vitest-pool-workers";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: "./wrangler.jsonc" },
    }),
  ],
  optimizeDeps: {
    exclude: ["@boundaryml/baml-bridge-web"],
  },
  test: {
    env: { BAML_TEST_RUNTIME: "workers" },
    include: ["workers/**/*.test.ts"],
    // Every pool worker is a full `workerd` runtime holding its own copy of
    // the wasm bridge — around half a gigabyte each. An uncapped pool takes
    // one per test file (17 were live at once here, 8 GB) and can exhaust
    // the machine alongside the other suites.
    maxWorkers: 4,
  },
});
