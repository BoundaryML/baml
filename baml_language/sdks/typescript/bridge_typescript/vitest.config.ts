import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    // A source module under test imports the native addon as `./native.js`,
    // which exists only in `dist/` once built; point it there so tests may
    // import `typescript_src/*` directly (and share the test realm's globals).
    alias: [
      {
        find: /^\.\/native\.js$/,
        replacement: fileURLToPath(new URL("./dist/native.js", import.meta.url)),
      },
    ],
  },
  test: {
    environment: "node",
    globals: true,
  },
});
