import { defineConfig } from 'tsup';

export default defineConfig({
  entry: ['src/index.ts'],
  format: ['cjs', 'esm'],
  dts: true,
  clean: true,
  external: [
    '@codemirror/autocomplete',
    '@codemirror/language',
    '@codemirror/legacy-modes',
    '@lezer/common',
    '@lezer/highlight',
    '@lezer/lr',
    '@uiw/codemirror-theme-vscode'
  ],
  noExternal: ['tslib'],
  outDir: 'dist',
  outExtension({ format }) {
    return {
      js: format === 'cjs' ? '.cjs' : '.js',
    };
  },
  splitting: false,
  sourcemap: true,
  target: 'es2020',
  tsconfig: './tsconfig.json',
});