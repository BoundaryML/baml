import { useEffect } from "react";
import BamlProjectManager from "./project_manager";
import { atom, useSetAtom, useAtomValue, useAtom } from 'jotai';
import { atomFamily } from 'jotai/utils';
import { WasmProject, WasmRuntime, WasmRuntimeContext, version as RuntimeVersion } from "@gloo-ai/baml-schema-wasm-web";
import { VSCodeButton } from "@vscode/webview-ui-toolkit/react";
import CustomErrorBoundary from "../utils/ErrorFallback";

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



const availableProjectsAtom = atom<string[]>([]);
const selectedProjectAtom = atom<string | null>(null);
const selectedFunctionAtom = atom<string | null>(null);
const selectedTestCaseAtom = atom<string | null>(null);
const runtimeCtx = atom(new WasmRuntimeContext())
const filesAtom = atom<Record<string, string>>({});
const projectAtom = atom<WasmProject | null>(null);
const runtimesAtom = atom<{
  last_successful_runtime?: WasmRuntime,
  current_runtime?: WasmRuntime
}>({});


const projectFamilyAtom = atomFamily((root_path: string) => projectAtom);
const runtimeFamilyAtom = atomFamily((root_path: string) => runtimesAtom);
const projectFilesAtom = atomFamily((root_path: string) => filesAtom);

const removeProjectAtom = atom(null, (get, set, root_path: string) => {
  set(projectFilesAtom(root_path), {});
  set(projectFamilyAtom(root_path), null);
  set(runtimeFamilyAtom(root_path), {});
  let availableProjects = get(availableProjectsAtom);
  set(availableProjectsAtom, availableProjects.filter(p => p !== root_path));
});

const updateFileAtom = atom(null, (get, set, { root_path, files }: { root_path: string, files: { name: string, content: string | undefined }[] }) => {
  let projFiles = get(projectFilesAtom(root_path));
  for (let file of files) {
    if (file.content === undefined) {
      delete projFiles[file.name];
    } else {
      projFiles[file.name] = file.content;
    }
  }

  let project = get(projectFamilyAtom(root_path))
  if (project) {
    for (let file of files) {
      project.update_file(file.name, file.content);
    }
  } else {
    project = WasmProject.new(root_path, files);
  }
  let rt = undefined;
  try {
    rt = project.runtime();
  } catch (e) {
    console.error(e);
  }

  let pastRuntime = get(runtimeFamilyAtom(root_path));
  let lastSuccessRt = pastRuntime.current_runtime ?? pastRuntime.last_successful_runtime;

  let availableProjects = get(availableProjectsAtom);
  if (!availableProjects.includes(root_path)) {
    set(availableProjectsAtom, [...availableProjects, root_path]);
  }

  set(projectFilesAtom(root_path), projFiles);
  set(projectAtom, project);
  set(runtimesAtom, { last_successful_runtime: lastSuccessRt, current_runtime: rt });
})

const selectedRuntimeAtom = atom((get) => {
  let project = get(selectedProjectAtom);
  if (!project) {
    return null;
  }

  let runtime = get(runtimeFamilyAtom(project));
  return runtime.current_runtime ?? runtime.last_successful_runtime;
});

export const versionAtom = atom((get) => RuntimeVersion());

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
          updateFile({ root_path: event.data.root_path, files: [{ name: event.data.name, content: event.data.content }] });
          break;
        case 'add_project':
          updateFile({ root_path: event.data.root_path, files: Object.entries(event.data.files).map(([name, content]) => ({ name, content })) });
          break;
        case 'remove_project':
          removeProject(event.data.root_path)
          break;
      }
    }

    window.addEventListener('message', fn);
    () => window.removeEventListener('message', fn);
  });

  return (
    <>
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