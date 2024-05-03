import wasmPack from 'vite-plugin-wasm-pack';
import topLevelAwait from "vite-plugin-top-level-await";
import wasm from 'vite-plugin-wasm'

import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

const isWatchMode = process.argv.includes('--watch') || true;
console.log('isWatchMode', isWatchMode);
// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    react(),
    wasm(),
    wasmPack("../../../../engine/baml-schema-wasm"),
    topLevelAwait()
  ],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  mode: isWatchMode ? 'development' : 'production',
  build: {
    minify: isWatchMode ? false : true,
    outDir: 'dist',
    sourcemap: isWatchMode ? 'inline' : undefined,
    rollupOptions: {
      // external: ['allotment/dist/index.css'],
      output: {
        entryFileNames: `assets/[name].js`,
        chunkFileNames: `assets/[name].js`,
        assetFileNames: `assets/[name].[ext]`,
      },
    },
  },
})
