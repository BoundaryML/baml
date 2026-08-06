import type { BamlPluginOptions, ProjectFactory } from './build-core.js';

interface BunBuild {
  onStart(callback: () => void | Promise<void>): void;
  onResolve(
    options: { filter: RegExp },
    callback: (args: {
      path: string;
      importer: string;
    }) =>
      | { path: string; namespace: string }
      | undefined
      | Promise<{ path: string; namespace: string } | undefined>,
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
  void build;
  void options;
  void factory;
  throw new Error('not implemented');
}
