import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    env: { BAML_TEST_RUNTIME: "node" },
    environment: "node",
    include: ["node/**/*.test.ts"],
  },
});
