import type { WasmBridge } from './backend.js';
import type { OpenProjectOptions } from './project.js';
import { BamlProject } from './project.js';

export interface LoadProjectOptions
  extends Omit<OpenProjectOptions, 'backend'> {
  backend?: 'native' | 'wasm' | 'auto' | import('./backend.js').ToolingBackend;
  wasm?: WasmBridge;
}

export async function loadProject(
  options: LoadProjectOptions,
): Promise<BamlProject> {
  void options;
  throw new Error('not implemented');
}
