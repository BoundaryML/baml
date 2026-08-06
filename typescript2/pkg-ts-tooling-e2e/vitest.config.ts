import { resolve } from 'node:path';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  resolve: {
    alias: [
      {
        find: '@boundaryml/baml-tooling/unplugin',
        replacement: resolve(
          import.meta.dirname,
          '../pkg-baml-tooling/src/unplugin.ts',
        ),
      },
      {
        find: /^@boundaryml\/baml-tooling$/,
        replacement: resolve(
          import.meta.dirname,
          '../pkg-baml-tooling/src/index.ts',
        ),
      },
    ],
  },
  test: {
    testTimeout: 30_000,
  },
});
