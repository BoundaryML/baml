#!/usr/bin/env node

const esbuildplugincopy = require('esbuild-plugin-copy')
const minify = process.argv.includes('--minify')
const sourcemap = process.argv.includes('--sourcemap')
const watch = process.argv.includes('--watch')
;(async () => {
  const ctx = await require('esbuild').context({
    entryPoints: ['./src/bin.ts'],
    bundle: true,
    outfile: './out/bin.js',
    minify: minify,
    sourcemap: sourcemap,
    external: ['vscode'],
    tsconfig: './tsconfig.json',
    format: 'cjs',
    platform: 'node',
    logLevel: 'info',
    plugins: [
      esbuildplugincopy.copy({
        assets: [
          {
            from: ['./node_modules/@gloo-ai/baml-schema-wasm/dist/*'],
            to: ['.'],
            watch: watch,
          },
        ],
      }),
    ],
  })
  if (watch) {
    await ctx.watch()
    console.log('watching...')
  } else {
    await ctx.rebuild()
    await ctx.dispose()
  }
})()
