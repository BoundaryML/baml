import { atom, getDefaultStore, useAtomValue, useSetAtom } from 'jotai';
import { atomFamily, atomWithStorage } from 'jotai/utils';
import { useEffect } from 'react';

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
import { standaloneFeatureFlagsAtom, isVSCodeEnvironment } from './feature-flags';

// ============================================================================
// WASM Panic Handling
// ============================================================================

export interface WasmPanicState {
  msg: string;
  timestamp: number;
}

// Atom to track WASM panics
export const wasmPanicAtom = atom<WasmPanicState | null>(null);

// Global setter function that will be wired up by useWasmPanicHandler
let globalSetPanic: ((msg: string) => void) | null = null;

// Set up the global panic handler BEFORE WASM loads
// This must be defined before wasmAtomAsync is evaluated
if (typeof window !== 'undefined') {
  (window as any).__onWasmPanic = (msg: string) => {
    console.error('[WASM Panic]', msg);

    // Call the setter if it's been wired up
    if (globalSetPanic) {
      globalSetPanic(msg);
    } else {
      console.warn('[WASM Panic] Handler called but atom setter not yet initialized');
    }
  };
}

/**
 * Hook to wire up the WASM panic handler to the Jotai atom.
 * Call this once in your root component to enable panic state tracking.
 *
 * @example
 * ```tsx
 * function App() {
 *   useWasmPanicHandler();
 *   return <YourApp />;
 * }
 * ```
 */
export const useWasmPanicHandler = () => {
  const setPanic = useSetAtom(wasmPanicAtom);

  useEffect(() => {
    // Wire up the global setter with telemetry
    globalSetPanic = (msg: string) => {
      const timestamp = Date.now();
      setPanic({ msg, timestamp });

      // Send telemetry about the panic
      vscode.sendTelemetry({
        action: 'wasm_panic',
        data: {
          panic_message: msg,
          timestamp,
          during_test_execution: false, // Will be overridden in test-runner if during test
        },
      });
    };

    // Cleanup on unmount
    return () => {
      globalSetPanic = null;
    };
  }, [setPanic]);
};

/**
 * Hook to clear panic state.
 * Use this to dismiss panic notifications in your UI.
 */
export const useClearWasmPanic = () => {
  const setPanic = useSetAtom(wasmPanicAtom);
  return () => setPanic(null);
};

// ============================================================================
// Feature Flags & Runtime
// ============================================================================

// Unified beta feature atom that works in both VS Code and standalone environments
export const betaFeatureEnabledAtom = atom((get) => {
  const isInVSCode = isVSCodeEnvironment();

  if (isInVSCode) {
    // In VSCode: try vscodeSettingsAtom, then bamlConfig fallback
    const vscodeSettings = get(vscodeSettingsAtom);
    if (vscodeSettings?.featureFlags) {
      return vscodeSettings.featureFlags.includes('beta');
    } else {
      // VSCode settings not loaded yet, use bamlConfig as immediate fallback
      const config = get(bamlConfig);
      return (config.config?.featureFlags ?? []).includes('beta');
    }
  } else {
    // Standalone: use standalone flags
    return get(standaloneFeatureFlagsAtom).includes('beta');
  }
});


let wasmAtomAsync = atom(async () => {
  const wasm = await import('@gloo-ai/baml-schema-wasm-web/baml_schema_build');
  // Enable WASM logging for debugging
  wasm.init_js_callback_bridge(vscode.loadAwsCreds, vscode.loadGcpCreds);
  return wasm;
},

  async (_get, set, newValue: null) => {
    set(wasmAtomAsync, null)
  }
);

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
    // Determine environment and get appropriate feature flags
    const isInVSCode = isVSCodeEnvironment();
    let featureFlags: string[];

    if (isInVSCode) {
      // In VSCode: try vscodeSettingsAtom, then bamlConfig fallback
      const vscodeSettings = get(vscodeSettingsAtom);
      if (vscodeSettings?.featureFlags) {
        featureFlags = vscodeSettings.featureFlags;
      } else {
        // VSCode settings not loaded yet, use bamlConfig as immediate fallback
        const config = get(bamlConfig);
        featureFlags = config.config?.featureFlags ?? [];
      }
    } else {
      // Standalone: use standalone flags
      featureFlags = get(standaloneFeatureFlagsAtom);
    }
    const rt = project.runtime(selectedEnvVars, featureFlags);
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

export const vscodeSettingsAtom = unwrap(
  atom(async (get) => {
    try {
      const settings = await vscode.getVSCodeSettings();
      return {
        enablePlaygroundProxy: settings.enablePlaygroundProxy,
        featureFlags: settings.featureFlags,
      };
    } catch (e) {
      console.error(
        `Error occurred while getting VSCode settings:\n${JSON.stringify(e)}`,
      );
      // Fallback to config if RPC fails
      const config = get(bamlConfig);
      return {
        enablePlaygroundProxy: config.config?.enablePlaygroundProxy ?? true,
        featureFlags: config.config?.featureFlags ?? [],
      };
    }
  }),
);

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