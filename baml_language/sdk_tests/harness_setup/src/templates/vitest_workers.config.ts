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
  },
});
