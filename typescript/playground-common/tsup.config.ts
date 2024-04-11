import { defineConfig } from "tsup";

const isProduction = process.env.NODE_ENV === "production";

export default defineConfig({
  clean: true,
  dts: true,
  entry: ["src/index.ts"],
  format: ["cjs", "esm"],
  external: ["react", "react/jsx-runtime"],
  minify: false,
  sourcemap: true,
  bundle: true,
  banner: { js: '"use client";' },
});