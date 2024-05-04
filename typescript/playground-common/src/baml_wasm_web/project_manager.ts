import BamlWasm, { WasmDiagnosticError } from '@gloo-ai/baml-schema-wasm-web'
import { readFile } from 'fs/promises'

type URI = {
  schema: string,
  fsPath: string
}

enum DiagnosticSeverity {
  Error = 1,
  Warning = 2,
  Information = 3,
  Hint = 4
}

type Diagnostic = {

}

type Notify = (params:
  { type: 'error' | 'warn' | 'info', message: string } |
  { type: 'diagnostic', errors: [string, Diagnostic[]][] } |
  { type: 'runtime_updated', root_path: string, files: Record<string, string> }
) => void

function findTopLevelParent(filePath: string): string | null {
  let currentPath = filePath;
  let parentDir: string | null = null;

  // Find the "baml_src" directory in the path
  let parts = currentPath.split('/')
  for (let i = parts.length - 1; i >= 0; i--) {
    if (parts[i] === 'baml_src') {
      return parts.slice(0, i + 1).join('/');
    }
  }

  return null;
}

const uriToRootPath = (uri: URI): string => {
  if (uri.schema !== 'file') {
    throw new Error(`URI schema is not file: ${uri}`)
  }

  // Find the "baml_src" directory in the path
  let found = findTopLevelParent(uri.fsPath);
  if (!found) {
    throw new Error(`No baml_src directory found in path: ${uri}`)
  }
  return found
}

class Project {
  private last_successful_runtime?: BamlWasm.WasmRuntime
  private current_runtime?: BamlWasm.WasmRuntime

  constructor(private files: BamlWasm.WasmProject, private ctx: BamlWasm.WasmRuntimeContext, private onSuccess: (e: WasmDiagnosticError, files: Record<string, string>) => void) {
  }

  update_runtime() {
    if (this.current_runtime == undefined) {
      try {
        this.current_runtime = this.files.runtime()
        this.onSuccess(this.files.diagnostics(this.current_runtime), Object.fromEntries(this.files.files().map((f): [string, string] => f.split("BAML_PATH_SPLTTER", 1) as [string, string])))
      } catch (e) {
        this.current_runtime = undefined
        throw e
      }
    }
  }

  runtime(): BamlWasm.WasmRuntime {
    let rt = this.current_runtime ?? this.last_successful_runtime;
    if (!rt) {
      throw new Error(`Project is not valid.`)
    }

    return rt
  }

  replace_all_files(files: BamlWasm.WasmProject) {
    this.files = files
    this.last_successful_runtime = this.current_runtime
    this.current_runtime = undefined
  }

  update_unsaved_file(file_path: string, content: string) {
    this.files.set_unsaved_file(file_path, content)
    if (this.current_runtime) {
      this.last_successful_runtime = this.current_runtime
    }
    this.current_runtime = undefined
  }

  save_file(file_path: string, content: string) {
    this.files.save_file(file_path, content)
    if (this.current_runtime) {
      this.last_successful_runtime = this.current_runtime
    }
    this.current_runtime = undefined
  }

  upsert_file(file_path: string, content: string | undefined) {
    this.files.update_file(file_path, content)
    if (this.current_runtime) {
      this.last_successful_runtime = this.current_runtime
    }
    this.current_runtime = undefined
  }

  // list_functions(): BamlWasm.WasmFunction[] {
  //   let runtime = this.runtime()

  //   return runtime.list_functions()
  // }

  // render_prompt(function_name: string, params: Record<string, any>): BamlWasm.WasmPrompt {
  //   let rt = this.runtime();
  //   let func = rt.get_function(function_name)
  //   if (!func) {
  //     throw new Error(`Function ${function_name} not found`)
  //   }

  //   return func.render_prompt(rt, this.ctx, params);
  // }
}

class BamlProjectManager {
  private projects: Map<string, Project> = new Map()

  constructor(private notifier?: Notify) {
    BamlWasm.enable_logs()
  }

  setNotifier(notifier: Notify) {
    this.notifier = notifier
  }

  private notify<T extends Parameters<Notify>['0']>(args: T) {
    if (this.notifier) {
      this.notifier(args)
    }
  }

  private handleMessage(e: any) {
    if (e instanceof BamlWasm.WasmDiagnosticError) {
      let diagnostics = new Map<string, Diagnostic[]>(e.all_files.map((f) => [f, []]))

      e.errors().forEach((err) => {
        if (err.type === 'error') {
          console.error(`${err.message}, ${err.start_line}, ${err.start_column}, ${err.end_line}, ${err.end_column}`)
        }
        diagnostics.get(err.file_path)!.push(
          {
            range: {
              start: {
                line: err.start_line,
                character: err.start_column
              },
              end: {
                line: err.end_line,
                character: err.end_column
              }
            },
            message: err.message,
            severity: err.type === 'error' ? DiagnosticSeverity.Error : DiagnosticSeverity.Warning,
            source: 'baml'
          });
      });
      this.notify({ errors: Array.from(diagnostics), type: 'diagnostic' })
    } else if (e instanceof Error) {
      this.notify({ message: e.message, type: 'error' })
    } else {
      this.notify({
        message: `${e}`,
        type: 'error'
      })
    }
  };

  private wrapSync<T>(fn: () => T): T | undefined {
    try {
      return fn()
    } catch (e) {
      this.handleMessage(e)
      return undefined
    }
  }

  private async wrapAsync<T>(fn: () => Promise<T>): Promise<T | undefined> {
    return await fn().catch(e => {
      this.handleMessage(e)
      return undefined
    })
  }


  static version(): string {
    return BamlWasm.version()
  }


  private get_project(root_path: string) {
    const project = this.projects.get(root_path);
    if (!project) {
      throw new Error(`Project not found for path: ${root_path}`)
    }

    return project
  }

  private add_project(root_path: string, files: { [path: string]: string }) {
    const project = BamlWasm.WasmProject.new(root_path, files)
    this.projects.set(root_path, new Project(project, new BamlWasm.WasmRuntimeContext(), (d, files: Record<string, string>) => {
      this.handleMessage(d)
      this.notify({ type: 'runtime_updated', root_path, files })
    }))
    return this.get_project(root_path)!
  }

  private remove_project(root_path: string) {
    this.projects.delete(root_path)
  }

  upsert_file(path: URI, content: string | undefined) {
    console.debug(`Upserting file: ${path}`)
    this.wrapSync(() => {
      let rootPath = uriToRootPath(path)
      if (this.projects.has(rootPath)) {
        let project = this.get_project(rootPath)
        project.upsert_file(path.fsPath, content)
        project.update_runtime()
      } else {
        console.debug(`Project not found for path: ${rootPath}`)
      }
    });
  }

  // Reload all files in a project
  // Takes in a URI to any file in the project
  replace_project_files(files: {
    path: URI,
    content: string
  }[]) {
    this.wrapSync(() => {
      if (files.length === 0) {
        this.notify({ type: 'warn', message: `Empty baml_src directory found. See Output panel -> BAML Language Server for more details.` })
      }

      let rootPath = uriToRootPath(files[0].path)
      let objFiles = Object.fromEntries(files.map(f => [f.path.fsPath, f.content]));
      if (!this.projects.has(rootPath)) {
        let project = this.add_project(rootPath, objFiles);
        project.update_runtime()
      } else {
        let project = this.get_project(rootPath)
        project.replace_all_files(BamlWasm.WasmProject.new(rootPath, objFiles))
        project.update_runtime()
      }
    });
  }
}

export default BamlProjectManager