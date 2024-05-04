'use client'

import { useEffect } from "react";
import BamlProjectManager from "./project_manager";
import { atom, useSetAtom, useAtomValue, useAtom } from 'jotai';
import { atomFamily, atomWithStorage, createJSONStorage } from 'jotai/utils';
import CustomErrorBoundary from "../utils/ErrorFallback";
import { VSCodeButton } from "@vscode/webview-ui-toolkit/react";
import type { WasmProject, WasmRuntimeContext, WasmRuntime, WasmDiagnosticError } from "@gloo-ai/baml-schema-wasm-web";

const wasm_loader = null;
const wasm = async (): Promise<typeof import("@gloo-ai/baml-schema-wasm-web")> => {
  if (!wasm_loader) {
    const wasm = await import("@gloo-ai/baml-schema-wasm-web");
    return wasm;
  }
  return wasm_loader;
}

// const wasm = await import("@gloo-ai/baml-schema-wasm-web");
// const { WasmProject, WasmRuntime, WasmRuntimeContext, version: RuntimeVersion } = wasm;

export const lintFn = async () => {
  const wasm = await import("@gloo-ai/baml-schema-wasm-web");
  return wasm.lint;
}

type Selection = {
  project?: string;
  function?: string;
  testCase?: string;
}

type ASTContextType = {
  projectMangager: BamlProjectManager;
  // Things associated with selection
  selected: Selection
}

const runtimeCtxRaw = atom<WasmRuntimeContext | null>(null)
const runtimeCtx = atom((get) => {
  let ctx = get(runtimeCtxRaw);
  if (!ctx) {
    throw new Error("WasmRuntimeContext was never called with set(...)");
  }
  return ctx;
});



const availableProjectsAtom = atom<string[]>([]);
const selectedProjectAtomRaw = atomWithStorage<string | null>('baml-selected-project', null, sessionStore);
const selectedFunctionAtomRaw = atomWithStorage<string | null>('baml-selected-function', null, sessionStore);
const selectedTestCaseAtomRaw = atomWithStorage<string | null>('baml-selected-testcase', null, sessionStore);
const filesAtom = atomWithStorage<Record<string, string>>('baml-files', {}, sessionStore);

export const selectedFunctionAtom = atom((get) => {
  let functions = get(availableFunctionsAtom);
  let name = get(selectedFunctionAtomRaw)
  if (functions.find(f => f.name == name) !== undefined) {
    return name
  }
  return functions.at(0)?.name ?? null
}, (get, set, name: string) => {
  set(selectedFunctionAtomRaw, name)
})
export const selectedProjectAtom = atom((get) => {
  let projects = get(availableProjectsAtom);
  let name = get(selectedProjectAtomRaw);
  if (projects.find(f => f === name)) {
    return name
  }
  return projects.at(0) ?? null
}, (get, set, name: string) => {
  set(selectedFunctionAtomRaw, name)
})
export const selectedTestCaseAtom = atom((get) => {
  let selected_function = get(selectedRtFunctionAtom);
  if (!selected_function) {
    return null
  }
  let test_case_name = get(selectedTestCaseAtomRaw);
  if (selected_function.test_cases.find(t => t.name === test_case_name) !== undefined) {
    return test_case_name
  }

  return selected_function.test_cases.at(0)?.name ?? null;
}, (get, set, test_case_name: string) => {
  set(selectedFunctionAtomRaw, test_case_name)
})

const projectAtom = atom<WasmProject | null>(null);
const runtimesAtom = atom<{
  last_successful_runtime?: WasmRuntime,
  current_runtime?: WasmRuntime
  diagnostics?: WasmDiagnosticError
}>({});
import deepEqual from 'fast-deep-equal'
import { sessionStore } from "./JotaiProvider";


const projectFamilyAtom = atomFamily((root_path: string) => projectAtom, deepEqual);
const runtimeFamilyAtom = atomFamily((root_path: string) => runtimesAtom, deepEqual);
export const projectFilesAtom = atomFamily((root_path: string) => filesAtom, deepEqual);

const removeProjectAtom = atom(null, (get, set, root_path: string) => {
  set(projectFilesAtom(root_path), {});
  set(projectFamilyAtom(root_path), null);
  set(runtimeFamilyAtom(root_path), {});
  let availableProjects = get(availableProjectsAtom);
  set(availableProjectsAtom, availableProjects.filter(p => p !== root_path));
});

export const updateFileAtom = atom(null, async (get, set, { reason, root_path, files, replace_all }: { reason: string, root_path: string, files: { name: string, content: string | undefined }[], replace_all?: true }) => {
  let _projFiles = get(projectFilesAtom(root_path));
  let project = get(projectFamilyAtom(root_path))

  let projFiles = {
    ..._projFiles
  }

  let filesNames = files.map(f => f.name);
  let filesToDelete = replace_all ? Object.keys(projFiles).filter(f => !filesNames.includes(f)) : [];

  if (project) {
    for (let file of files) {
      project.update_file(file.name, file.content);
    }
    for (let f of filesToDelete) {
      project.update_file(f, undefined);
    }
  } else {
    projFiles = Object.fromEntries(files.filter(f => f.content !== undefined).map(f => [f.name, f.content as string]));
    let rsFiles = Object.fromEntries(files.filter(f => f.content !== undefined && f.name.startsWith(root_path)).map(f => [f.name, f.content]));
    project = (await wasm()).WasmProject.new(root_path, rsFiles);
    console.log("Created new project", project);
  }
  if (replace_all) {
    projFiles = Object.fromEntries(files.filter(f => f.content !== undefined).map(f => [f.name, f.content as string]));
  } else {
    for (let file of files) {
      if (file.content === undefined) {
        delete projFiles[file.name];
      } else {
        projFiles[file.name] = file.content;
      }
    }
  }

  let rt = undefined;
  let diag = undefined;
  try {
    rt = project.runtime();
    diag = project.diagnostics(rt);
  } catch (e) {
    let WasmDiagnosticError = (await wasm()).WasmDiagnosticError;
    if (e instanceof Error) {
      console.error(e.message);
    } else if (e instanceof WasmDiagnosticError) {
      diag = e;
    } else {
      console.error(e);
    }
  }

  let pastRuntime = get(runtimeFamilyAtom(root_path));
  let lastSuccessRt = pastRuntime.current_runtime ?? pastRuntime.last_successful_runtime;

  let availableProjects = get(availableProjectsAtom);
  if (!availableProjects.includes(root_path)) {
    console.log("Adding project", root_path);
    set(availableProjectsAtom, [...availableProjects, root_path]);
  }

  console.log("Updated project", reason);
  set(projectFilesAtom(root_path), projFiles);
  set(projectFamilyAtom(root_path), project);
  set(runtimeFamilyAtom(root_path), { last_successful_runtime: lastSuccessRt, current_runtime: rt, diagnostics: diag });
})

const selectedRuntimeAtom = atom((get) => {
  let project = get(selectedProjectAtom);
  if (!project) {
    return null;
  }

  let runtime = get(runtimeFamilyAtom(project));
  return runtime.current_runtime ?? runtime.last_successful_runtime;
});

export const versionAtom = atom(async (get) => {
  return (await wasm()).version();
});

export const availableFunctionsAtom = atom((get) => {
  let runtime = get(selectedRuntimeAtom);
  if (!runtime) {
    return [];
  }

  return runtime.list_functions(get(runtimeCtx));
});

export const selectedRtFunctionAtom = atom((get) => {
  let allFunctions = get(availableFunctionsAtom);
  let func = get(selectedFunctionAtom);
  if (!func) {
    return null;
  }

  return allFunctions.find(f => f.name === func) ?? null;
});



export const selectedRtTestCaseAtom = atom((get) => {
  let func = get(selectedRtFunctionAtom);
  let test_case = get(selectedTestCaseAtom);
  if (!func || !test_case) {
    return null;
  }

  return func.test_cases.find(tc => tc.name === test_case) ?? null;
});


export const renderPromptAtom = atom((get) => {
  let runtime = get(selectedRuntimeAtom);
  let func = get(selectedRtFunctionAtom);
  let test_case = get(selectedRtTestCaseAtom);

  if (!runtime || !func || !test_case) {
    return null;
  }

  let params = Object.fromEntries(test_case.inputs.map((input) => [input.name, input.value]));

  return func.render_prompt(runtime, get(runtimeCtx), params);
})

// We don't use ASTContext.provider because we should the default value of the context
export const EventListener: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const updateFile = useSetAtom(updateFileAtom);
  const removeProject = useSetAtom(removeProjectAtom);
  const availableProjects = useAtomValue(availableProjectsAtom);
  const [selectedProject, setSelectedProject] = useAtom(selectedProjectAtom);
  const setRuntimeCtx = useSetAtom(runtimeCtxRaw);
  const version = useAtomValue(versionAtom);

  useEffect(() => {
    let fn = (event: MessageEvent<{
      command: 'modify_file',
      root_path: string,
      name: string,
      content: string | undefined
    } | {
      command: 'add_project',
      root_path: string,
      files: Record<string, string>
    } | {
      command: 'remove_project',
      root_path: string
    }>) => {
      switch (event.data.command) {
        case 'modify_file':
          updateFile({ reason: 'modify_file', root_path: event.data.root_path, files: [{ name: event.data.name, content: event.data.content }] });
          break;
        case 'add_project':
          updateFile({ reason: 'add_project', root_path: event.data.root_path, files: Object.entries(event.data.files).map(([name, content]) => ({ name, content })), replace_all: true });
          break;
        case 'remove_project':
          removeProject(event.data.root_path)
          break;
      }
    }

    window.addEventListener('message', fn);
    wasm().then((w) => {
      setRuntimeCtx((prev) => {
        if (prev) {
          return prev
        } else {
          return new w.WasmRuntimeContext()
        }
      })
    });

    () => window.removeEventListener('message', fn);
  });

  return (
    <>
      <div className="absolute bottom-2 right-2 text-xs bg-background pl-2 pt-1">BAML Version: {version}</div>
      {selectedProject === null ? (
        availableProjects.length === 0 ? (
          <div>
            No baml projects loaded yet.
            <br />
            Open a baml file or wait for the extension to finish loading!
          </div>
        ) : (
          <div>
            <h1>Projects</h1>
            <div>
              {availableProjects.map((root_dir) => (
                <div key={root_dir}>
                  <VSCodeButton
                    onClick={() => setSelectedProject(root_dir)}
                  >
                    {root_dir}
                  </VSCodeButton>
                </div>
              ))}
            </div>
          </div>
        )
      ) : (
        <CustomErrorBoundary>
          {children}
        </CustomErrorBoundary>
      )}
    </>
  )
}