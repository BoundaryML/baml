import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    globals: true,
    mockReset: true,
    clearMocks: true,
    alias: {
      vscode: new URL('./src/__mocks__/vscode.ts', import.meta.url).pathname,
    },
  },
});