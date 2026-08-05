import type { BamlPluginOptions, ProjectFactory } from './build-core.js';
import { BamlBuildCore } from './build-core.js';

interface BunBuild {
  onStart(callback: () => void | Promise<void>): void;
  onResolve(
    options: { filter: RegExp },
    callback: (args: {
      path: string;
      importer: string;
    }) => { path: string; namespace: string } | undefined,
  ): void;
  onLoad(
    options: { filter: RegExp; namespace: string },
    callback: (args: {
      path: string;
    }) => { contents: string; loader: 'js' } | undefined,
  ): void;
}

export function setup(
  build: BunBuild,
  options: BamlPluginOptions = {},
  factory?: ProjectFactory,
): void {
  const core = new BamlBuildCore(options, factory);
  build.onStart(() => core.start());
  build.onResolve(
    { filter: /(?:\.baml$|^baml:client$|^\\0baml:)/ },
    ({ path, importer }) => {
      const resolved = core.resolve(path, importer);
      return resolved ? { namespace: 'baml', path: resolved } : undefined;
    },
  );
  build.onLoad({ filter: /.*/, namespace: 'baml' }, ({ path }) => {
    const loaded = core.load(path);
    return loaded ? { contents: loaded.code, loader: 'js' } : undefined;
  });
}
