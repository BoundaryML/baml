import { normalizePath } from 'vite'

import * as path from 'node:path';
import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';
import wasm from 'vite-plugin-wasm';
import { viteStaticCopy } from 'vite-plugin-static-copy'
import { consoleForwardPlugin } from "vite-console-forward-plugin";
import jotaiReactRefresh from 'jotai/babel/plugin-react-refresh'
import jotaiDebugLabel from 'jotai/babel/plugin-debug-label'
import tailwindcss from '@tailwindcss/vite';
import { visualizer } from 'rollup-plugin-visualizer';
const isWatchMode = process.argv.includes('--watch');
const analyzeBundle = process.env.ANALYZE === 'true';
const srcPath = normalizePath(path.resolve(__dirname, './dist/'));
const destPath = normalizePath(path.resolve(__dirname, '../vscode-ext/dist/playground'));

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    react({ babel: { plugins: [jotaiDebugLabel, jotaiReactRefresh] } }),
    wasm(),
    viteStaticCopy({
      targets: [
        {
          src: srcPath,
          dest: destPath
        }
      ]
    }),
    // Forward browser console logs to the terminal in dev mode
    // !isWatchMode && consoleForwardPlugin(),
    tailwindcss(),
    // topLevelAwait(),
    // Bundle analyzer - run with ANALYZE=true pnpm build
    analyzeBundle && visualizer({
      open: true,
      filename: 'bundle-stats.html',
      gzipSize: true,
      brotliSize: true,
    }),
  ].filter(Boolean),
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
      overlay: false, // Disable error overlay for HMR failures
    },
    watch: {
      usePolling: true,
      interval: 100,
      ignored: [
        // Ignore node_modules to reduce watcher overhead
        '**/node_modules/**',
      ],
    },
  },
  // css: {
  //   // Force full reload for CSS changes instead of HMR
  //   devSourcemap: true,
  // },
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
        entryFileNames: 'assets/[name].js',
        chunkFileNames: 'assets/[name].js',
        assetFileNames: 'assets/[name].[ext]',
      },
    },
  },
  optimizeDeps: {
    esbuildOptions: {
      target: 'esnext',
    },
  },
});
