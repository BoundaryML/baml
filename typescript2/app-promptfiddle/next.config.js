import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { PHASE_DEVELOPMENT_SERVER } from 'next/constants.js';

const projectDir = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

// Worker entry files from @codingame/monaco-vscode-api use ES module syntax
// (import/export). Webpack copies them as raw assets via the `new URL()`
// pattern. Next.js's TerserPlugin parses all .js assets as scripts and chokes
// on import/export. Patch the TerserPlugin's optimize method to mark worker
// assets as already-minimized so they get skipped.
try {
  const terserPath = require.resolve(
    'next/dist/build/webpack/plugins/terser-webpack-plugin/src/index.js',
  );
  const { TerserPlugin } = require(terserPath);
  const origOptimize = TerserPlugin.prototype.optimize;
  TerserPlugin.prototype.optimize = async function (
    compiler,
    compilation,
    assets,
    ...rest
  ) {
    for (const [name, info] of compilation.assetsInfo) {
      if (/worker/i.test(name) && /\.js$/i.test(name)) {
        compilation.assetsInfo.set(name, { ...info, minimized: true });
      }
    }
    return origOptimize.call(this, compiler, compilation, assets, ...rest);
  };
} catch (e) {
  console.warn('[next.config] Could not patch TerserPlugin:', e.message);
}

/** @type {import('next').NextConfig} */
export default function nextConfig(phase) {
  return {
    // Keep dev and production builds isolated so a running dev server
    // can't corrupt the manifests that `next build` needs to finalize.
    distDir: phase === PHASE_DEVELOPMENT_SERVER ? '.next-dev' : '.next-build',
    experimental: {
      typedRoutes: true,
    },
    reactStrictMode: true,
    transpilePackages: ['pkg-editor', 'pkg-playground', 'pkg-proto'],
    webpack: (config, { webpack }) => {
      config.resolve = config.resolve || {};
      config.resolve.alias = {
        ...config.resolve.alias,
        'pkg-editor': path.resolve(projectDir, '../pkg-editor/src'),
        'pkg-playground': path.resolve(projectDir, '../pkg-playground/src'),
        'pkg-proto': path.resolve(projectDir, '../pkg-proto/src'),
      };

      // Enable WASM support for bridge_wasm
      config.experiments = {
        ...config.experiments,
        asyncWebAssembly: true,
      };

      config.module.rules.push({
        test: /\.baml$/,
        type: 'asset/source',
      });

      // just-bash/browser bundle still references a few Node.js built-ins
      // (gzip/gunzip commands — dead code in browser). Webpack 5 treats
      // "node:X" as an unhandled URI scheme and bails before resolve runs.
      // Rewrite node:-prefixed imports to bare names so resolve.fallback
      // can map them to empty modules.
      config.plugins.push(
        new webpack.NormalModuleReplacementPlugin(/^node:/, (resource) => {
          resource.request = resource.request.replace(/^node:/, '');
        }),
      );
      config.resolve.fallback = {
        ...config.resolve.fallback,
        zlib: false,
      };

      return config;
    },
  };
}
