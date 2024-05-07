import path from 'path'
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
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
      '@gloo-ai/baml-schema-wasm-web': path.resolve(__dirname, '../../../baml-schema-wasm-web/dist'),
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
