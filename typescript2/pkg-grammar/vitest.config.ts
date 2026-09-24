import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["tests/**/*.test.ts"],
    reporters: process.env.CI
      ? [
          "default",
          ["junit", { addFileAttribute: true, outputFile: "./junit.xml" }],
        ]
      : ["default"],
  },
});
