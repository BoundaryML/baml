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

// The VS Code extension's getWebviewHtml() hardcodes /assets/index.css and the
// playground server cache-busts that exact name. monaco-vscode-api contributes
// dozens of `index.css` chunk assets (one per service-override barrel), so the
// app's own entry stylesheet no longer reliably wins the `index.css` filename.
// This plugin renames the ENTRY chunk's CSS to a stable `assets/index.css`
// (identified via Vite's per-chunk importedCss metadata, not by filename) and
// rewrites the emitted index.html to match. Deterministic regardless of how
// many monaco chunks are present.
const stableEntryCssPlugin = () => ({
  name: 'stable-entry-css',
  enforce: 'post' as const,
  generateBundle(_options: any, bundle: Record<string, any>) {
    const STABLE = 'assets/index.css';
    const entry = Object.values(bundle).find(
      (c) => c.type === 'chunk' && c.isEntry,
    );
    const importedCss: Set<string> | undefined = entry?.viteMetadata?.importedCss;
    if (!entry || !importedCss || importedCss.size === 0) return;

    // The entry graph imports a single stylesheet (src/styles.css → tailwind +
    // app styles); lazy chunks (Monaco) carry their own CSS separately.
    const [oldName] = [...importedCss];
    if (oldName === STABLE) return;
    const asset = bundle[oldName];
    if (!asset) return;

    delete bundle[oldName];
    asset.fileName = STABLE;
    bundle[STABLE] = asset;
    importedCss.delete(oldName);
    importedCss.add(STABLE);

    // Patch any references in the emitted HTML (Vite injects the entry CSS link
    // during its own generateBundle, which has already run at enforce:'post').
    for (const chunk of Object.values(bundle)) {
      if (chunk.type === 'asset' && chunk.fileName.endsWith('.html')) {
        const src =
          typeof chunk.source === 'string'
            ? chunk.source
            : Buffer.from(chunk.source).toString('utf8');
        chunk.source = src.split(oldName).join(STABLE);
      }
    }
  },
});

export default defineConfig({
  plugins: [react(), tailwindcss(), wasmNoCachePlugin(), stableEntryCssPlugin()],
  resolve: {
    dedupe: ['react', 'react-dom', 'monaco-editor', 'vscode', '@codingame/monaco-vscode-api'],
    alias: {
      // pkg-editor/pkg-playground are aliased to SOURCE (outside this project's
      // root), so their bare `react` imports resolve to their own node_modules
      // symlink — a distinct module id from the app's react. dedupe alone
      // doesn't collapse it, so the lazy editor chunk ends up with a SECOND
      // React instance whose hook dispatcher is null → React #321 ("invalid
      // hook call") the moment the editor mounts. Pin react/react-dom (and the
      // jsx-runtime subpath, matched via the `react` prefix) to one absolute
      // path so every importer shares a single instance.
      react: resolve(projectRoot, 'node_modules/react'),
      'react-dom': resolve(projectRoot, 'node_modules/react-dom'),
      '@b/pkg-editor': resolve(projectRoot, '../pkg-editor/src'),
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
  optimizeDeps: {
    // monaco-vscode-api ships ESM that esbuild's dep pre-bundling mangles;
    // exclude it (and the WASM bridge) so it's loaded as-authored.
    exclude: ['@b/bridge_wasm', '@codingame/monaco-vscode-api', 'vscode', 'monaco-editor'],
    esbuildOptions: {
      target: 'esnext',
    },
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
  build: {
    // monaco-vscode-api uses top-level await and other modern syntax.
    target: 'esnext',
    rollupOptions: {
      output: {
        // The VS Code extension's getWebviewHtml() hardcodes /assets/index.js
        // and /assets/index.css, and the playground server cache-busts those
        // exact names — so the ENTRY js/css must stay stably named. Everything
        // else (incl. monaco-vscode-api's many `index<N>` chunks) is hashed so
        // it can't collide with — and steal — the entry's `index.css` name.
        // Entry JS stays stable for the VS Code webview HTML; the entry CSS is
        // stabilized to assets/index.css by stableEntryCssPlugin (above).
        // Everything else is hashed so Monaco's many `index*` chunks can't
        // collide with the entry assets.
        entryFileNames: 'assets/index.js',
        chunkFileNames: 'assets/[name]-[hash].js',
        assetFileNames: 'assets/[name]-[hash][extname]',
      }
    }
  },
  define: {
    __DEV__: process.env.NODE_ENV !== 'production'
  }
});
