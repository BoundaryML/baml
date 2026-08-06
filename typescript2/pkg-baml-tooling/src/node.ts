import { createRequire } from 'node:module';
import { isAbsolute, join, resolve } from 'node:path';
import {
  type NativeAddon,
  NativeBackend,
  type ToolingBackend,
  WasmBackend,
  type WasmBridge,
} from './backend.js';
import { BamlProject, type OpenProjectOptions } from './project.js';

export interface LoadProjectOptions
  extends Omit<OpenProjectOptions, 'backend'> {
  backend?: 'native' | 'wasm' | 'auto' | ToolingBackend;
  wasm?: WasmBridge;
}

export async function loadProject(
  options: LoadProjectOptions,
): Promise<BamlProject> {
  const backend =
    typeof options.backend === 'object'
      ? options.backend
      : selectBackend(options);
  return BamlProject.open({ ...options, backend });
}

function selectBackend(options: LoadProjectOptions): ToolingBackend {
  if (options.backend === 'wasm') {
    if (!options.wasm)
      throw new Error(
        'WASM backend requested without a BamlToolingBridge instance',
      );
    return new WasmBackend(options.wasm);
  }
  try {
    const require = createRequire(join(process.cwd(), 'package.json'));
    const configuredBridge = process.env.BAML_TOOLING_BRIDGE_PATH;
    const bridgeSpecifier = configuredBridge
      ? isAbsolute(configuredBridge)
        ? configuredBridge
        : resolve(process.env.INIT_CWD ?? process.cwd(), configuredBridge)
      : '@boundaryml/baml-bridge-tooling';
    return new NativeBackend(require(bridgeSpecifier) as NativeAddon);
  } catch (error) {
    if (options.backend === 'native') throw error;
    if (options.wasm) return new WasmBackend(options.wasm);
    throw new Error(
      'No BAML tooling backend is installed; install @boundaryml/baml-bridge-tooling or provide the WASM backend',
      { cause: error },
    );
  }
}
