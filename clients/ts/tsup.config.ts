import { defineConfig } from "tsup";

const isProduction = process.env.NODE_ENV === "production";

export default defineConfig({
  clean: true,
  dts: true,
  entry: ["src/index.ts",
    "src/deserializer/deserializer.ts",
    "src/client_manager/index.ts",
    "src/ffi_layer.ts",
    "src/baml_test_runner/index.ts",
    "src/deserializer/diagnostics.ts"],
  format: ["cjs", "esm"],
  minify: false,
  sourcemap: false,
  // external: ["jest-resolve/build/default_resolver"]
});