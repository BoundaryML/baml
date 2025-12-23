import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = dirname(fileURLToPath(import.meta.url));

// Vite configuration for the VSCode extension webview
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      // Point to source for hot reload support
      '@baml/playground-common': resolve(projectRoot, '../pkg-playground/src'),
    },
  },
  server: {
    port: 5173,
  },
  define: {
    __DEV__: process.env.NODE_ENV !== 'production',
  },
});
