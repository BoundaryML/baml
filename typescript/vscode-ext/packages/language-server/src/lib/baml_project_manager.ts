import { URI } from 'vscode-uri'
import BamlWasm, { WasmDiagnosticError } from '@gloo-ai/baml-schema-wasm-node'
import { DiagnosticSeverity, Diagnostic } from 'vscode-languageserver'
import { findTopLevelParent, gatherFiles } from '../file/fileUtils'
import { readFile } from 'fs/promises'

type Notify = (params:
  { type: 'error' | 'warn' | 'info', message: string } |
  { type: 'diagnostic', errors: [string, Diagnostic[]][] } |
  { type: 'runtime_updated', root_path: string, files: Record<string, string> }
) => void

const uriToRootPath = (uri: URI): string => {
  // Find the "baml_src" directory in the path
  if (uri.scheme !== 'file') {
    throw new Error(`Unsupported scheme: ${uri.scheme}`)
  }
  let found = findTopLevelParent(uri.fsPath);
  if (!found) {
    throw new Error(`No baml_src directory found in path: ${uri.fsPath}`)
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
        const files = this.files.files()
        const fileMap = Object.fromEntries(files.map((f): [string, string] => f.split("BAML_PATH_SPLTTER", 2) as [string, string]))
        this.onSuccess(this.files.diagnostics(this.current_runtime), fileMap)
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
  private static instance: BamlProjectManager | null = null

  private projects: Map<string, Project> = new Map()

  constructor(private notifier: Notify) {
    BamlWasm.enable_logs()
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
      this.notifier({ errors: Array.from(diagnostics), type: 'diagnostic' })
    } else if (e instanceof Error) {
      this.notifier({ message: e.message, type: 'error' })
    } else {
      this.notifier({
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
    this.projects.set(root_path, new Project(project, new BamlWasm.WasmRuntimeContext(), (d, files) => {
      this.handleMessage(d)
      this.notifier({ type: 'runtime_updated', root_path, files })
    }))
    return this.get_project(root_path)!
  }

  private remove_project(root_path: string) {
    this.projects.delete(root_path)
  }

  async upsert_file(path: URI, content: string | undefined) {
    console.debug(`Upserting file: ${path}`)
    await this.wrapAsync(async () => {
      let rootPath = uriToRootPath(path)
      if (this.projects.has(rootPath)) {
        let project = this.get_project(rootPath)
        project.upsert_file(path.fsPath, content)
        project.update_runtime()
      } else {
        await this.reload_project_files(path)
      }
    });
  }

  async save_file(path: URI, content: string) {
    console.debug(`Saving file: ${path}`)
    await this.wrapAsync(async () => {
      let rootPath = uriToRootPath(path)
      if (this.projects.has(rootPath)) {
        let project = this.get_project(rootPath)
        project.save_file(path.fsPath, content)
        project.update_runtime()
      } else {
        await this.reload_project_files(path)
      }
    });
  }

  update_unsaved_file(path: URI, content: string) {
    console.debug(`Updating unsaved file: ${path}`)
    this.wrapSync(() => {
      let rootPath = uriToRootPath(path)
      let project = this.get_project(rootPath)
      project.update_unsaved_file(path.fsPath, content)
      project.update_runtime()
    });
  }

  async touch_project(path: URI) {
    await this.wrapAsync(async () => {
      let rootPath = uriToRootPath(path)
      if (!this.projects.has(rootPath)) {
        await this.reload_project_files(path)
      }
    })
  }

  // Reload all files in a project
  // Takes in a URI to any file in the project
  async reload_project_files(path: URI) {
    console.debug(`Reloading project files: ${path}`)
    await this.wrapAsync(async () => {
      let rootPath = uriToRootPath(path)

      let files = await Promise.all(gatherFiles(rootPath).map(async (uri): Promise<[string, string]> => {
        let path = uri.fsPath
        let content = await readFile(path, 'utf8')
        return [path, content]
      }));

      if (files.length === 0) {
        this.notifier({ type: 'warn', message: `Empty baml_src directory found: ${rootPath}. See Output panel -> BAML Language Server for more details.` })
      }

      if (!this.projects.has(rootPath)) {
        let project = this.add_project(rootPath, Object.fromEntries(files));
        project.update_runtime()
      } else {
        let project = this.get_project(rootPath)
        project.replace_all_files(BamlWasm.WasmProject.new(rootPath, Object.fromEntries(files)))
        project.update_runtime()
      }
    });
  }
}

export default BamlProjectManager