import { basename } from 'node:path';
import { createUnplugin } from 'unplugin';
import {
  BamlBuildCore,
  type BamlPluginOptions,
  type ProjectFactory,
} from './build-core.js';

export function createBamlUnplugin(factory?: ProjectFactory) {
  return createUnplugin<BamlPluginOptions>((options = {}) => {
    const core = new BamlBuildCore(options, factory);
    return {
      buildEnd() {
        core.close();
      },
      async buildStart() {
        await core.start();
      },
      enforce: 'pre',
      load(id) {
        const result = core.load(id);
        for (const path of result?.watchFiles ?? []) this.addWatchFile(path);
        return result ? { code: result.code, map: null } : null;
      },
      name: 'boundaryml-baml',
      async resolveId(id, importer) {
        return core.resolve(id, importer);
      },
      vite: {
        configureServer() {
          core.enableDevelopment();
        },
        handleHotUpdate(context) {
          // Non-BAML files are not ours: returning undefined lets the
          // default HMR pipeline (react-refresh, etc.) handle them. The
          // config gate is a basename match — the build core re-checks the
          // exact config path, and a suffix match ('mybaml.toml') would
          // reach the Rust session and be rejected.
          if (
            !context.file.endsWith('.baml') &&
            basename(context.file) !== 'baml.toml'
          )
            return undefined;
          const modules: typeof context.modules = [];
          let fullReload = false;
          core.watchChange(context.file, undefined, {
            development: true,
            fullReload() {
              fullReload = true;
              context.server.ws.send({ type: 'full-reload' });
            },
            invalidate(id) {
              const module = context.server.moduleGraph.getModuleById(id);
              if (!module) return;
              context.server.moduleGraph.invalidateModule(module);
              modules.push(module);
            },
            watchError: (error) =>
              warnPluginContext(this, 'handleHotUpdate', error),
          });
          return fullReload ? [] : modules;
        },
      },
      watchChange(id) {
        core.watchChange(id, undefined, {
          watchError: (error) => warnPluginContext(this, 'watchChange', error),
        });
      },
    };
  });
}

export const baml = createBamlUnplugin();

// Watch-hook contexts are typed without `warn`, but rollup and vite provide
// it at runtime; a rejected watch update is reported there and never thrown.
function warnPluginContext(
  context: unknown,
  hook: string,
  error: unknown,
): void {
  (context as { warn?: (message: string) => void }).warn?.(
    `boundaryml-baml ${hook}: ${error instanceof Error ? error.message : String(error)}`,
  );
}

export type {
  BamlPluginOptions,
  BuildHost,
  ProjectFactory,
} from './build-core.js';
export { BamlBuildCore } from './build-core.js';
export default baml;
