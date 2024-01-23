import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'
const isWatchMode = process.argv.includes('--watch');
console.log('isWatchMode', isWatchMode);
// https://vitejs.dev/config/
export default defineConfig({
  plugins: react(),
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
