import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = dirname(fileURLToPath(import.meta.url));

// Vite configuration for the standalone playground shell
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      'pkg-playground': resolve(projectRoot, '../pkg-playground/src')
    }
  },
  server: {
    port: 5173,
    cors: true,
    headers: {
      'Access-Control-Allow-Origin': '*',
    },
  },
  build: {
    rollupOptions: {
      output: {
        // Use consistent names for the output files so the VSCode extension can find them
        entryFileNames: 'assets/index.js',
        chunkFileNames: 'assets/[name].js',
        assetFileNames: 'assets/[name].[ext]'
      }
    }
  },
  define: {
    __DEV__: process.env.NODE_ENV !== 'production'
  }
});
