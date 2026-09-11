import { defineConfig } from "vitest/config";

const packageDir = "typescript2/pkg-grammar";

export default defineConfig({
  test: {
    root: "../..",
    include: [`${packageDir}/tests/**/*.test.ts`],
    reporters: process.env.CI
      ? [
          "default",
          [
            "junit",
            {
              addFileAttribute: true,
              outputFile: `./${packageDir}/junit.xml`,
            },
          ],
        ]
      : ["default"],
  },
});
