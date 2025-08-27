import { normalizePath } from 'vite'

import * as path from 'node:path';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import wasm from 'vite-plugin-wasm';
import { viteStaticCopy } from 'vite-plugin-static-copy'

const isWatchMode = process.argv.includes('--watch');
const srcPath = normalizePath(path.resolve(__dirname, './dist/'));
const destPath = normalizePath(path.resolve(__dirname, '../vscode-ext/dist/playground'));

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    react({
      babel: {
        presets: ['jotai/babel/preset'],
      },
    }),
    wasm(),
    viteStaticCopy({
      targets: [
        {
          src: srcPath,
          dest: destPath
        }
      ]
    })
    // topLevelAwait(),
  ],
  // root: path.resolve(process.cwd(), './src'),
  server: {
    strictPort: true, // Allow fallback to next available port
    port: 3030,
    host: true,
    cors: {
      origin: '*',
    },
    headers: {
      'Access-Control-Allow-Origin': '*',
    },
    hmr: {
      // This is needed for HMR to work in VSCode webviews
      protocol: 'ws',
      host: 'localhost',
    },
    watch: {
      usePolling: true,
      interval: 100,
    },
  },
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      '~': path.resolve(__dirname, './src'),
      '@gloo-ai/baml-schema-wasm-web': path.resolve(
        __dirname,
        '../../../engine/baml-schema-wasm/web/dist',
      ),
      baml_wasm_web: path.resolve(
        __dirname,
        '../../../engine/baml-schema-wasm/web/dist',
      ),
    },
  },
  mode: isWatchMode ? 'development' : 'production',
  build: {
    target: 'esnext',
    minify: isWatchMode ? false : 'esbuild',
    sourcemap: isWatchMode ? 'inline' : false,
    rollupOptions: {
      external: ['baml_wasm_web/rpc'],
      output: {
        format: 'es',
        entryFileNames: 'assets/[name]-[hash].js',
        chunkFileNames: 'assets/[name]-[hash].js',
        assetFileNames: 'assets/[name]-[hash].[ext]',
      },
    },
  },
  optimizeDeps: {
    esbuildOptions: {
      target: 'esnext',
    },
  },
});
