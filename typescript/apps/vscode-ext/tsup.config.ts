import { defineConfig } from 'tsup';

export default defineConfig({
  entry: ['src/extension.ts'],
  outDir: 'dist',
  outExtension: () => ({ js: '.js' }),
  target: 'node18',
  format: ['cjs'],
  external: [
    'vscode',
    // Node.js built-ins
    'assert',
    'buffer',
    'child_process',
    'crypto',
    'events',
    'fs',
    'http',
    'https',
    'net',
    'os',
    'path',
    'process',
    'readline',
    'stream',
    'string_decoder',
    'url',
    'util',
    'zlib'
  ],
  bundle: true,
  minify: process.env.CI === 'true' ? true : false,
  sourcemap: process.env.CI === 'true' ? false : true,
  // We need to disable clean in CI because we want to keep the dist folder which has the baml-cli in it
  clean: false,
  platform: 'node',
  splitting: false,
  treeshake: true,
  // Force bundling of all dependencies except those explicitly marked as external
  noExternal: [
    // Bundle all dependencies except Node.js built-ins and vscode
    /^(?!vscode$|assert$|buffer$|child_process$|crypto$|events$|fs$|http$|https$|net$|os$|path$|process$|readline$|stream$|string_decoder$|url$|util$|zlib$)/
  ],
});
