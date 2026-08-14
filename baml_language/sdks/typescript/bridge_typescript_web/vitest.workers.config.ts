import { cloudflareTest } from "@cloudflare/vitest-pool-workers";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: "./wrangler.test.jsonc" },
    }),
  ],
  resolve: {
    alias: {
      "#bridge-web-core": fileURLToPath(new URL("./dist/workerd-wasm/bridge_web_core.js", import.meta.url)),
      "#bridge-web-native": fileURLToPath(new URL("./dist/workerd/native.js", import.meta.url)),
    },
  },
  test: {
    include: ["tests/*.test.ts"],
  },
});
