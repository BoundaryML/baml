import { atom, useAtomValue } from 'jotai';
import { atomFamily, atomWithStorage } from 'jotai/utils';

import type {
  WasmDiagnosticError,
  WasmRuntime,
} from '@gloo-ai/baml-schema-wasm-web/baml_schema_build';
import { unwrap } from 'jotai/utils';
import { bamlConfig } from '../../baml_wasm_web/bamlConfig';
import { vscodeLocalStorageStore } from './Jotai';
import { orchIndexAtom } from './playground-panel/atoms-orch-graph';
import type { ICodeBlock } from './types';
import { vscode } from './vscode';

const wasmAtomAsync = atom(async () => {
  const wasm = await import('@gloo-ai/baml-schema-wasm-web/baml_schema_build');
  wasm.init_js_callback_bridge(vscode.loadAwsCreds, vscode.loadGcpCreds);
  return wasm;
});

export const wasmAtom = unwrap(wasmAtomAsync);

export const useWaitForWasm = () => {
  const wasm = useAtomValue(wasmAtom);
  return wasm !== undefined;
};

export const filesAtom = atom<Record<string, string>>({});
export const sandboxFilesAtom = atom<Record<string, string>>({});

export const projectAtom = atom((get) => {
  const wasm = get(wasmAtom);
  const files = get(filesAtom);
  if (wasm === undefined) {
    return undefined;
  }
  // filter out files that are not baml files
  const bamlFiles = Object.entries(files).filter(([path, content]) =>
    path.endsWith('.baml'),
  );
  // TODO: add python generator if using sandbox

  return wasm.WasmProject.new('./', bamlFiles);
});

export const ctxAtom = atom((get) => {
  const wasm = get(wasmAtom);
  if (wasm === undefined) {
    return undefined;
  }
  const context = new wasm.WasmCallContext();
  const orch_index = get(orchIndexAtom);
  context.node_index = orch_index;
  return context;
});

export const runtimeAtom = atom<{
  rt: WasmRuntime | undefined;
  diags: WasmDiagnosticError | undefined;
  lastValidRt: WasmRuntime | undefined;
}>((get) => {
  try {
    const wasm = get(wasmAtom);
    const project = get(projectAtom);
    const envVars = get(envVarsAtom);

    if (wasm === undefined || project === undefined) {
      const previousState: {
        rt: WasmRuntime | undefined;
        diags: WasmDiagnosticError | undefined;
        lastValidRt: WasmRuntime | undefined;
      } = get(runtimeAtom);
      return {
        rt: undefined,
        diags: undefined,
        lastValidRt: previousState.lastValidRt,
      };
    }
    const selectedEnvVars = Object.fromEntries(
      Object.entries(envVars).filter(([key, value]) => value !== undefined),
    );
    const rt = project.runtime(selectedEnvVars);
    const diags = project.diagnostics(rt);
    return { rt, diags, lastValidRt: rt };
  } catch (e) {
    console.log('Error occurred while getting runtime', e);
    const wasm = get(wasmAtom);
    if (wasm) {
      const WasmDiagnosticError = wasm.WasmDiagnosticError;
      if (e instanceof WasmDiagnosticError) {
        const previousState: {
          rt: WasmRuntime | undefined;
          diags: WasmDiagnosticError | undefined;
          lastValidRt: WasmRuntime | undefined;
        } = get(runtimeAtom);
        return {
          rt: undefined,
          diags: e,
          lastValidRt: previousState.lastValidRt,
        };
      }
    }
    if (e instanceof Error) {
      console.error(e.message);
    } else {
      console.error(e);
    }
  }
  return { rt: undefined, diags: undefined, lastValidRt: undefined };
});

export const diagnosticsAtom = atom((get) => {
  const runtime = get(runtimeAtom);
  return runtime.diags?.errors() ?? [];
});

export const numErrorsAtom = atom((get) => {
  const errors = get(diagnosticsAtom);

  const warningCount = errors.filter((e) => e.type === 'warning').length;

  return { errors: errors.length - warningCount, warnings: warningCount };
});

// todo debounce this.
export const generatedFilesAtom = atom((get) => {
  const project = get(projectAtom);
  if (project === undefined) {
    return undefined;
  }
  const runtime = get(runtimeAtom);
  if (runtime.rt === undefined) {
    return undefined;
  }

  const generators = project.run_generators();
  const files = generators.flatMap((gen) =>
    gen.files.map((f) => ({
      path: f.path_in_output_dir,
      content: f.contents,
      outputDir: gen.output_dir,
    })),
  );
  return files;
});

export const generatedFilesByLangAtom = atomFamily(
  (lang: ICodeBlock['language']) =>
    atom((get) => {
      const allFiles = get(generatedFilesAtom);
      if (!allFiles) return undefined;

      return allFiles
        .filter((f) => f.outputDir.includes(lang))
        .map(({ path, content }) => ({
          path,
          content,
        }));
    }),
);

export const isPanelVisibleAtom = atom(false);

const vscodeSettingsAtom = atom<{ enablePlaygroundProxy: boolean }>((get) => {
  const config = get(bamlConfig);
  return {
    enablePlaygroundProxy: config.config?.enablePlaygroundProxy ?? true,
  };
});

const playgroundPortAtom = unwrap(
  atom(async () => {
    try {
      const res = await vscode.getPlaygroundPort();
      return res;
    } catch (e) {
      console.error(
        `Error occurred while getting playground port:\n${JSON.stringify(e)}`,
      );
      return 0;
    }
  }),
);

export const proxyUrlAtom = atom((get) => {
  const vscodeSettings = get(vscodeSettingsAtom);
  const port = get(playgroundPortAtom);
  const proxyUrl = port && port !== 0 ? `http://localhost:${port}` : undefined;
  const proxyEnabled = !!vscodeSettings?.enablePlaygroundProxy;
  return {
    proxyEnabled,
    proxyUrl,
  };
});

export const resetEnvKeyValuesAtom = atom(null, (get, set) => {
  set(envKeyValueStorage, []);
});
export const envKeyValuesAtom = atom(
  (get) => {
    const envKeyValues = get(envKeyValueStorage);
    return envKeyValues.map(([k, v], idx): [string, string, number] => [
      k,
      v,
      idx,
    ]);
  },
  (
    get,
    set,
    update: // Update value
      | { itemIndex: number; value: string }
      // Update key
      | { itemIndex: number; newKey: string }
      // Remove key
      | { itemIndex: number; remove: true }
      // Insert key
      | {
          itemIndex: null;
          key: string;
          value?: string;
        },
  ) => {
    if (update.itemIndex !== null) {
      const keyValues = [...get(envKeyValueStorage)];
      const targetItem = keyValues[update.itemIndex];
      if (targetItem) {
        if ('value' in update) {
          targetItem[1] = update.value ?? '';
        } else if ('newKey' in update) {
          targetItem[0] = update.newKey;
        } else if ('remove' in update) {
          keyValues.splice(update.itemIndex, 1);
        }
      }
      set(envKeyValueStorage, keyValues);
    } else {
      set(envKeyValueStorage, (prev) => [
        ...prev,
        [update.key, update.value ?? ''],
      ]);
    }
  },
);

// Simple atom for user's environment variables (direct editing)
export const userEnvVarsAtom = atom(
  (get) => {
    const envKeyValues = get(envKeyValuesAtom);
    return Object.fromEntries(
      envKeyValues
        .map(([k, v]) => [k, v])
        .filter(([k]) => k !== 'BOUNDARY_PROXY_URL'),
    );
  },
  (get, set, newEnvVars: Record<string, string>) => {
    const envKeyValues = Object.entries(newEnvVars);
    set(envKeyValueStorage, envKeyValues);
  },
);

// Computed atom that includes proxy logic (for runtime usage)
export const envVarsAtom = atom(
  (get) => {
    if (typeof window === 'undefined') {
      return {};
    }

    // Check for Next.js environment
    const isNextJs = !!(window as any).next?.version;

    if (isNextJs) {
      // NextJS environment - check proxy settings but use Next.js specific proxy URL
      const { proxyEnabled } = get(proxyUrlAtom);
      const userEnvVars = get(userEnvVarsAtom);

      if (!proxyEnabled) {
        return userEnvVars;
      }

      // Proxy enabled - use Next.js specific proxy URL
      const nextJsProxyUrl = window?.location?.origin?.includes('localhost')
        ? 'https://fiddle-proxy.fly.dev' // localhost development
        : 'https://fiddle-proxy.fly.dev'; // production

      return {
        ...userEnvVars,
        BOUNDARY_PROXY_URL: nextJsProxyUrl,
      };
    }

    const { proxyEnabled, proxyUrl } = get(proxyUrlAtom);
    const userEnvVars = get(userEnvVarsAtom);

    if (!proxyEnabled) {
      // if proxy is not enabled, just return user vars without BOUNDARY_PROXY_URL
      return userEnvVars;
    }

    if (proxyUrl === undefined) {
      return userEnvVars;
    }

    // Add or update BOUNDARY_PROXY_URL based on current proxy settings
    return {
      ...userEnvVars,
      BOUNDARY_PROXY_URL: proxyUrl,
    };
  },
  // Delegate writes to userEnvVarsAtom to avoid interference
  (get, set, newEnvVars: Record<string, string>) => {
    const { BOUNDARY_PROXY_URL, ...userVars } = newEnvVars;
    set(userEnvVarsAtom, userVars);
  },
);

export const requiredEnvVarsAtom = atom((get) => {
  const { rt } = get(runtimeAtom);
  if (rt === undefined) {
    return [];
  }
  const requiredEnvVars = rt.required_env_vars();
  const defaultEnvVars = ['OPENAI_API_KEY', 'ANTHROPIC_API_KEY'];
  for (const e of defaultEnvVars) {
    if (!requiredEnvVars.find((envVar) => e === envVar)) {
      requiredEnvVars.push(e);
    }
  }

  return requiredEnvVars;
});

const defaultEnvKeyValues: [string, string][] = (() => {
  if (typeof window === 'undefined') {
    return [];
  }
  if ((window as any).next?.version) {
    console.log('Running in nextjs');

    const domain = window?.location?.origin || '';
    if (domain.includes('localhost')) {
      // we can do somehting fancier here later if we want to test locally.
      return [['BOUNDARY_PROXY_URL', 'https://fiddle-proxy.fly.dev']];
    }
    return [['BOUNDARY_PROXY_URL', 'https://fiddle-proxy.fly.dev']];
  }
  console.log('Not running in a Next.js environment, set default value');
  // Not running in a Next.js environment, set default value
  return [['BOUNDARY_PROXY_URL', 'http://localhost:0000']];
})();
export const envKeyValueStorage = atomWithStorage<[string, string][]>(
  'env-key-values',
  defaultEnvKeyValues,
  vscodeLocalStorageStore,
);
