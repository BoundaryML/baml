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
      resolveId(id, importer) {
        return core.resolve(id, importer);
      },
      watchChange(id) {
        core.watchChange(id, undefined);
      },
    };
  });
}

export const baml = createBamlUnplugin();

export type {
  BamlPluginOptions,
  BuildHost,
  ProjectFactory,
} from './build-core.js';
export { BamlBuildCore } from './build-core.js';
export default baml;
