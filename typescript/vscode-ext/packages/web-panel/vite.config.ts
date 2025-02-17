import * as path from 'path'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'
import topLevelAwait from 'vite-plugin-top-level-await'
import wasm from 'vite-plugin-wasm'

const isWatchMode = process.argv.includes('--watch') || true
// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    react({
      babel: {
        presets: ['jotai/babel/preset'],
      },
    }),
    wasm(),
    topLevelAwait(),
  ],
  server: {
    port: process.env.VITE_PORT ? parseInt(process.env.VITE_PORT) : 3000,
    strictPort: false, // Allow fallback to next available port
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
      '@gloo-ai/baml-schema-wasm-web': path.resolve(__dirname, '../../../baml-schema-wasm-web/dist'),
      '~': path.resolve(__dirname, './src'),
      baml_wasm_web: path.resolve(__dirname, '../../../baml-schema-wasm-web/dist'),
    },
  },
  mode: isWatchMode ? 'development' : 'production',
  build: {
    target: 'esnext',
    minify: isWatchMode ? false : 'esbuild',
    outDir: 'dist',
    sourcemap: isWatchMode ? 'inline' : false,
    rollupOptions: {
      external: ['baml_wasm_web/rpc'],
      output: {
        format: 'es',
        entryFileNames: `assets/[name].js`,
        chunkFileNames: `assets/[name].js`,
        assetFileNames: `assets/[name].[ext]`,
      },
    },
  },
  optimizeDeps: {
    esbuildOptions: {
      target: 'esnext',
    },
  },
})
