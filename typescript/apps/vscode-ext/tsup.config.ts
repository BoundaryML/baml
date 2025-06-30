import { defineConfig } from 'tsup';

export default defineConfig({
  entry: ['src/extension.ts'],
  outDir: 'dist',
  outExtension: () => ({ js: '.js' }),
  target: 'node18',
  format: ['cjs'],
  external: ['vscode'],
  bundle: true,
  clean: true,
  platform: 'node',
  splitting: false,
  treeshake: true,
});
