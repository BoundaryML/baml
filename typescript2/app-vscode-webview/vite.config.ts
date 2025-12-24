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
    port: 5173
  },
  define: {
    __DEV__: process.env.NODE_ENV !== 'production'
  }
});
