import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = dirname(fileURLToPath(import.meta.url));

// Vite configuration for the standalone playground shell
// Plugin to disable caching for WASM files
const wasmNoCachePlugin = () => ({
  name: 'wasm-no-cache',
  configureServer(server: any) {
    server.middlewares.use((req: any, res: any, next: any) => {
      if (req.url?.includes('.wasm') || req.url?.includes('bridge_wasm') || req.url?.includes('@b/bridge_wasm')) {
        res.setHeader('Cache-Control', 'no-store, no-cache, must-revalidate');
        res.setHeader('Pragma', 'no-cache');
        res.setHeader('Expires', '0');
      }
      next();
    });
  },
});

export default defineConfig({
  plugins: [react(), tailwindcss(), wasmNoCachePlugin()],
  resolve: {
    alias: {
      '@b/pkg-playground': resolve(projectRoot, '../pkg-playground/src'),
      '@b/pkg-proto': resolve(projectRoot, '../pkg-proto/src'),
      '@b/bridge_wasm': resolve(projectRoot, '../pkg-playground/wasm/bridge_wasm.js'),
    }
  },
  worker: {
    // monaco-languageclient workers use dynamic imports (code-splitting),
    // which requires ES module format instead of the default iife.
    format: 'es',
  },
  server: {
    port: 4000,
    strictPort: true,
    cors: true,
    headers: {
      'Access-Control-Allow-Origin': '*',
    },
    watch: {
      // Watch the WASM output directory for hot reload
      ignored: ['!**/pkg-playground/wasm/**'],
    },
  },
  optimizeDeps: {
    // Don't pre-bundle the WASM package so changes are picked up immediately
    exclude: ['@b/bridge_wasm'],
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
