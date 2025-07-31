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
import { apiKeysAtom } from '../../components/api-keys-dialog/atoms';

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

// Simple hash function for file content
const hashString = (str: string): number => {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash = hash & hash; // Convert to 32-bit integer
  }
  return hash;
};

// Helper to create a stable key from baml files for memoization
const createBamlFilesKey = (bamlFiles: [string, string][]): string => {
  return bamlFiles
    .map(([path, content]) => `${path}:${hashString(content)}`)
    .sort()
    .join('|');
};

// Cache for memoizing project creation
let projectCache: {
  key: string;
  project: any;
} | null = null;

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
  
  // Create a key representing the current baml files state
  const currentKey = createBamlFilesKey(bamlFiles);
  
  // Return cached project if files haven't changed
  if (projectCache && projectCache.key === currentKey) {
    return projectCache.project;
  }
  
  // TODO: add python generator if using sandbox
  const newProject = wasm.WasmProject.new('./', bamlFiles);
  
  // Update cache
  projectCache = {
    key: currentKey,
    project: newProject,
  };
  
  return newProject;
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
    const apiKeys = get(apiKeysAtom);

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
      Object.entries(apiKeys).filter(([key, value]) => value !== undefined),
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
  const files = generators.flatMap((gen: any) =>
    gen.files.map((f: any) => ({
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
        .filter((f: any) => f.outputDir.includes(lang))
        .map(({ path, content }: { path: string; content: string }) => ({
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
