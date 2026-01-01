import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      'pkg-playground': resolve(projectRoot, '../pkg-playground/src'),
      'baml-runtime-wasm': resolve(projectRoot, '../pkg-playground/wasm/baml_runtime_wasm.js'),
    },
  },
  test: {
    globals: true,
    setupFiles: ['./vitest.setup.browser.ts'],
    include: ['src/**/*.browser.test.{ts,tsx}'],
    browser: {
      enabled: true,
      provider: 'playwright',
      name: 'chromium',
      headless: true,
    },
  },
  define: {
    __DEV__: true,
  },
});
