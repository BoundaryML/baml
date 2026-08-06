import type { BamlPluginOptions, ProjectFactory } from './build-core.js';

export function createBamlUnplugin(factory?: ProjectFactory): never {
  void factory;
  throw new Error('not implemented');
}

export const baml: ReturnType<typeof createBamlUnplugin> =
  undefined as never;

export type {
  BamlPluginOptions,
  BuildHost,
  ProjectFactory,
} from './build-core.js';
export { BamlBuildCore } from './build-core.js';
export default baml;
