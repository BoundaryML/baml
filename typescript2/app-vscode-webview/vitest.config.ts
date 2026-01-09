import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { playwright } from '@vitest/browser-playwright';

const projectRoot = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@b/pkg-playground': resolve(projectRoot, '../pkg-playground/src'),
      '@b/baml-playground-wasm': resolve(projectRoot, '../pkg-playground/wasm/baml_playground_wasm.js'),
    },
  },
  define: {
    __DEV__: true,
  },
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'unit',
          globals: true,
          environment: 'jsdom',
          setupFiles: ['./vitest.setup.ts'],
          include: ['src/**/*.test.{ts,tsx}'],
          exclude: ['src/**/*.browser.test.{ts,tsx}', 'src/**/*.hmr.test.{ts,tsx}'],
          css: true,
          browser: {
            enabled: false,
          },
        },
      },
      {
        extends: true,
        test: {
          name: 'browser',
          globals: true,
          setupFiles: ['./vitest.setup.browser.ts'],
          include: ['src/**/*.browser.test.{ts,tsx}'],
          browser: {
            enabled: true,
            provider: playwright(),
            instances: [{ browser: 'chromium' }],
            headless: true,
          },
        },
      },
      {
        extends: true,
        test: {
          name: 'hmr',
          globals: true,
          include: ['src/**/*.hmr.test.ts'],
          globalTimeout: 30_000, // 30 seconds max for entire test suite
          testTimeout: 120_000, // 2 minutes for WASM rebuilds
          hookTimeout: 60_000, // 1 minute for setup/teardown
          // Run sequentially - these tests modify shared state (Rust source files)
          pool: 'forks',
          singleFork: true,
          // Retry once in case of flaky timing
          retry: 1,
        },
      },
    ],
  },
});
