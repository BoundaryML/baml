import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'
import wasm from 'vite-plugin-wasm'
import topLevelAwait from 'vite-plugin-top-level-await'

const isWatchMode = process.argv.includes('--watch') || true;
console.log('isWatchMode', isWatchMode);

console.log("dirname is ", __dirname)
// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    react(),
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
