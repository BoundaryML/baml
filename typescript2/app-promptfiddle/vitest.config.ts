import { defineProject } from 'vitest/config';

export default defineProject({
  test: {
    name: 'hmr',
    globals: true,
    include: ['src/**/*.hmr.test.ts'],
    testTimeout: 300_000, // 5 minutes for WASM rebuilds
    hookTimeout: 120_000, // 2 minutes for setup/teardown
    // Run sequentially - these tests modify shared state (Rust source files)
    pool: 'forks',
    poolOptions: {
      forks: {
        singleFork: true,
      },
    },
    // Retry once in case of flaky timing
    retry: 1,
  },
});
