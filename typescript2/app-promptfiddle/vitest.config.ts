import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: 'hmr',
          globals: true,
          include: ['src/**/*.hmr.test.ts'],
          globalTimeout: 300_000, // 5 minutes max for entire test suite
          testTimeout: 300_000, // 5 minutes for WASM rebuilds
          hookTimeout: 120_000, // 2 minutes for setup/teardown
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
