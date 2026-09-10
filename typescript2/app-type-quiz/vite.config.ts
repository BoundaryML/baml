import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

const here = dirname(fileURLToPath(import.meta.url));

// The bridge is linked from the BAML tree rather than installed from the
// registry, because the generated SDK is built by the compiler in this
// checkout. Its WebAssembly therefore lives outside this app, and the dev
// server serves nothing outside its root unless told which paths are meant.
const bridge = resolve(
  here,
  '../../baml_language/sdks/typescript/bridge_typescript_web',
);

export default defineConfig({
  build: {
    // The BAML runtime and the formatter are both WebAssembly, and both are
    // large enough that Vite's size warning would be noise rather than news.
    chunkSizeWarningLimit: 8000,
    target: 'esnext',
  },
  // The bridge instantiates its WebAssembly with a top-level await, so both
  // the dependency pre-bundle and the build output have to allow one.
  optimizeDeps: { esbuildOptions: { target: 'esnext' } },
  plugins: [react()],
  resolve: {
    alias: {
      '@quiz/fmt': resolve(here, 'wasm/fmt_wasm.js'),
      // Both are generated or built, never edited, and both are gitignored:
      // `pnpm prepare:deps` produces them.
      '@quiz/sdk': resolve(here, 'src/baml_sdk/index.ts'),
    },
  },
  server: {
    fs: { allow: [here, resolve(here, '..'), bridge] },
  },
});
