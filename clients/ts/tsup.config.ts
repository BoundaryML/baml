import { defineConfig } from "tsup";

const isProduction = process.env.NODE_ENV === "production";

export default defineConfig({
  clean: true,
  dts: true,
  // we can't split them into chunks yet cause the ffi_layer.ts gets initialized in each dependent chunk and messes up the tracing state. Until that is fixed we just bundle it up into one file, and set fake export paths in the package.json pretending we have many different directories.
  entry: ["src/index.ts"],
  format: ["cjs", "esm"],
  minify: false,
  sourcemap: false,
  splitting: false,
  // cjsInterop: true,
});