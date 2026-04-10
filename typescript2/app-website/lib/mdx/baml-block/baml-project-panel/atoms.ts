import { type Diagnostic } from '@codemirror/lint';
import { atom, useAtomValue } from 'jotai';
import { atomFamily, atomWithStorage } from 'jotai/utils';

import { unwrap } from 'jotai/utils';
import { type ICodeBlock } from './types';

const wasmAtomAsync = atom(async () => {
  const wasm = await import('@gloo-ai/baml-schema-wasm-web/baml_schema_build');
  return wasm;
});

export const wasmAtom = unwrap(wasmAtomAsync);

export const useWaitForWasm = () => {
  const wasm = useAtomValue(wasmAtom);
  return wasm !== undefined;
};

export const filesAtom = atom<string>('');
export const sandboxFilesAtom = atom<string>('');

const pythonGenerator = `
generator python {
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet"
    output_type "python/pydantic"
    
    // Where the generated code will be saved (relative to baml_src/)
    output_dir "python"
    
    // What interface you prefer to use for the generated code (sync/async)
    // Both are generated regardless of the choice, just modifies what is exported
    // at the top level
    default_client_mode "sync"
    
    // Version of runtime to generate code for (should match installed baml-py version)
    version "0.66.0"
}

generator typescript {
    // Valid values: "python/pydantic", "typescript", "ruby/sorbet"
    output_type "typescript"
    
    // Where the generated code will be saved (relative to baml_src/)
    output_dir "typescript"
    
    // Version of runtime to generate code for (should match installed baml-py version)
    version "0.66.0"
}

`;
export const projectAtom = atom((get) => {
  const wasm = get(wasmAtom);
  let files = get(filesAtom);
  if (wasm === undefined) {
    return undefined;
  }
  files = files + pythonGenerator;
  return wasm.WasmProject.new('./', { './baml_src/main.baml': files });
});

export const ctxAtom = atom((get) => {
  const wasm = get(wasmAtom);
  if (wasm === undefined) {
    return undefined;
  }
  return new wasm.WasmCallContext();
});

export const runtimeAtom = atom((get) => {
  const wasm = get(wasmAtom);
  const project = get(projectAtom);
  const envVars = get(envVarsAtom);
  if (wasm === undefined || project === undefined) {
    return { rt: undefined, diags: undefined };
  }
  try {
    const selectedEnvVars = Object.fromEntries(
      Object.entries(envVars).filter(([key, value]) => value !== undefined),
    );
    const rt = project.runtime(selectedEnvVars);
    const diags = project.diagnostics(rt);
    return { rt, diags };
  } catch (e) {
    const WasmDiagnosticError = wasm.WasmDiagnosticError;
    if (e instanceof Error) {
      console.error(e.message);
    } else if (e instanceof WasmDiagnosticError) {
      return { rt: undefined, diags: e };
    } else {
      console.error(e);
    }
  }
  return { rt: undefined, diags: undefined };
});

export const diagnosticsAtom = atom((get) => {
  const runtime = get(runtimeAtom);
  return runtime.diags?.errors() ?? [];
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

export const CodeMirrorDiagnosticsAtom = atom((get) => {
  const diags = get(diagnosticsAtom);
  return diags.map((d): Diagnostic => {
    return {
      from: d.start_ch,
      to: d.start_ch === d.end_ch ? d.end_ch + 1 : d.end_ch,
      message: d.message,
      severity: d.type === 'warning' ? 'warning' : 'error',
      source: 'baml',
      markClass:
        d.type === 'error'
          ? 'decoration-wavy decoration-red-500 text-red-450 stroke-blue-500'
          : 'decoration-wavy decoration-yellow-500 text-yellow-450 stroke-blue-500',
    };
  });
});

export const isPanelVisibleAtom = atom(false);

export const envVarsAtom = atomWithStorage<{
  [key: string]: string | undefined;
}>('baml-env-vars', {
  BOUNDARY_PROXY_URL: 'https://fiddle-proxy.fly.dev',
});

export const requiredEnvVarsAtom = atom((get) => {
  const { rt } = get(runtimeAtom);
  if (rt === undefined) {
    return [];
  }
  const requiredEnvVars = rt.required_env_vars();

  const defaultEnvVars = ['OPENAI_API_KEY', 'ANTHROPIC_API_KEY'];
  defaultEnvVars.forEach((e) => {
    if (!requiredEnvVars.find((envVar) => e === envVar)) {
      requiredEnvVars.push(e);
    }
  });

  return requiredEnvVars;
});
