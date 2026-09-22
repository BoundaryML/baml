import { execFileSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

const here = dirname(fileURLToPath(import.meta.url));

// The bridge is linked from the BAML tree rather than installed from the
// registry, because the generated SDK is built by the compiler in this
// checkout. Its WebAssembly therefore lives outside this app, and the dev
// server serves nothing outside its root unless told which paths are meant.
/**
 * The commit this build is of, stamped into every transcript a learner
 * downloads so a file that comes back later says which bank made its cases.
 * The deploy job builds bridge, SDK and app from one checkout and passes the
 * commit in; a local build reads it from git.
 */
function commitOf(): string {
  const given = process.env.TYPE_QUIZ_COMMIT;
  if (given !== undefined && given !== '') {
    return given;
  }
  try {
    return execFileSync('git', ['rev-parse', 'HEAD'], {
      encoding: 'utf8',
    }).trim();
  } catch {
    return 'unknown';
  }
}

const bridge = resolve(
  here,
  '../../baml_language/sdks/typescript/bridge_typescript_web',
);

export default defineConfig({
  // Relative, so the built page works from any path: the repository's Pages
  // site is shared with another tool, and this one lives under `type-quiz/`.
  base: './',
  build: {
    // The BAML runtime and the formatter are both WebAssembly, and both are
    // large enough that Vite's size warning would be noise rather than news.
    chunkSizeWarningLimit: 8000,
    target: 'esnext',
  },
  define: {
    __TYPE_QUIZ_COMMIT__: JSON.stringify(commitOf()),
  },
  // The bridge instantiates its WebAssembly with a top-level await, so both
  // the dependency pre-bundle and the build output have to allow one.
  optimizeDeps: { esbuildOptions: { target: 'esnext' } },
  plugins: [react()],
  resolve: {
    alias: {
      '@quiz/fmt': resolve(here, 'wasm/baml_fmt_wasm.js'),
      // Both are generated or built, never edited, and both are gitignored:
      // `pnpm prepare:deps` produces them.
      '@quiz/sdk': resolve(here, 'src/baml_sdk/index.ts'),
    },
  },
  server: {
    fs: { allow: [here, resolve(here, '..'), bridge] },
  },
});
