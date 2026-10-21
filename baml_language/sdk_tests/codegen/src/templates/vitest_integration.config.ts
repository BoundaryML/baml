import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["worker_startup.test.ts"],
    hookTimeout: 60_000,
    testTimeout: 60_000,
  },
});
