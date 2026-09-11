import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import react from '@vitejs/plugin-react';
import { playwright } from '@vitest/browser-playwright';
import { defineConfig } from 'vitest/config';

const projectRoot = dirname(fileURLToPath(import.meta.url));
const packageDir = 'typescript2/app-vscode-webview';

export default defineConfig({
  define: {
    __DEV__: true,
  },
  // Pre-bundle every dependency the browser tests pull in. Without this, a
  // cold run (fresh checkout in CI) discovers dependencies mid-run,
  // re-optimizes, and force-reloads the tester page - aborting browser test
  // collection ("No test suite found" / "browser has been closed" depending
  // on phase). The vitest tester entry does not include the test files, so
  // the startup dep scan misses their import graph; pointing entries at the
  // tests makes the scan traverse it (through the pkg-playground source
  // alias) and prebundle up front. A bare include list cannot express the
  // aliased sibling package's deps: pnpm's strict layout makes them
  // unresolvable from this project root ("Failed to resolve dependency"),
  // so only the app's own devDeps ride include.
  optimizeDeps: {
    entries: [
      resolve(projectRoot, 'src/**/*.browser.test.{ts,tsx}'),
      resolve(projectRoot, 'vitest.setup.browser.ts'),
    ],
    include: ['@testing-library/jest-dom/vitest', '@testing-library/react'],
  },
  plugins: [react()],
  resolve: {
    alias: {
      '@b/bridge_wasm': resolve(
        projectRoot,
        '../pkg-playground/wasm/bridge_wasm.js',
      ),
      '@b/pkg-playground': resolve(projectRoot, '../pkg-playground/src'),
    },
  },
  test: {
    root: '../..',
    reporters: process.env.CI
      ? [
          'default',
          [
            'junit',
            {
              addFileAttribute: true,
              outputFile: `./${packageDir}/junit.xml`,
            },
          ],
        ]
      : ['default'],
    projects: [
      {
        extends: true,
        test: {
          browser: {
            enabled: false,
          },
          css: true,
          environment: 'jsdom',
          exclude: [`${packageDir}/src/**/*.browser.test.{ts,tsx}`],
          globals: true,
          include: [`${packageDir}/src/**/*.test.{ts,tsx}`],
          name: 'unit',
          setupFiles: [resolve(projectRoot, 'vitest.setup.ts')],
        },
      },
      {
        extends: true,
        test: {
          browser: {
            enabled: true,
            headless: true,
            instances: [{ browser: 'chromium' }],
            provider: playwright(),
          },
          globals: true,
          include: [`${packageDir}/src/**/*.browser.test.{ts,tsx}`],
          name: 'browser',
          setupFiles: [resolve(projectRoot, 'vitest.setup.browser.ts')],
        },
      },
    ],
  },
});
