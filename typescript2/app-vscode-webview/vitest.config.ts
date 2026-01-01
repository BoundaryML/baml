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
    environment: 'jsdom',
    setupFiles: ['./vitest.setup.ts'],
    include: ['src/**/*.test.{ts,tsx}'],
    css: true,
  },
  define: {
    __DEV__: true,
  },
});
