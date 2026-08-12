import { defineConfig } from 'tsup';
import { cpSync, existsSync, mkdirSync } from 'fs';
import { resolve } from 'path';

function stagePlayground() {
  const src = resolve(__dirname, '../app-vscode-webview/dist');
  const dest = resolve(__dirname, 'dist/playground');
  if (existsSync(src)) {
    mkdirSync(dest, { recursive: true });
    cpSync(src, dest, { recursive: true });
  }
}

export default defineConfig((options) => ({
  entry: ['src/extension.ts'],
  outDir: 'dist',
  outExtension: () => ({ js: '.js' }),
  target: 'node18',
  format: ['cjs'],
  external: ['vscode'],
  bundle: true,
  noExternal: [/^(?!vscode$)/],
  sourcemap: true,
  clean: !options.watch,
  platform: 'node',
  splitting: false,
  treeshake: true,
  onSuccess: async () => {
    stagePlayground();
  },
}));
