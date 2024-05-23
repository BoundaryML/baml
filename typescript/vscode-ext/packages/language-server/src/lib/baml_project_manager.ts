import BamlWasm, { type WasmDiagnosticError } from '@gloo-ai/baml-schema-wasm-node'
import { readFile } from 'fs/promises'
import { type Diagnostic, DiagnosticSeverity, Position, LocationLink, Hover } from 'vscode-languageserver'
import { TextDocument } from 'vscode-languageserver-textdocument' 
import { readFileSync } from 'fs';

import type { URI } from 'vscode-uri'
import { findTopLevelParent, gatherFiles } from '../file/fileUtils'
import { getWordAtPosition, convertDocumentTextToTrimmedLineArray, trimLine } from './ast'



type Notify = (
  params:
    | { type: 'error' | 'warn' | 'info'; message: string }
    | { type: 'diagnostic'; errors: [string, Diagnostic[]][] }
    | { type: 'runtime_updated'; root_path: string; files: Record<string, string> },
) => void

const uriToRootPath = (uri: URI): string => {
  // Find the "baml_src" directory in the path
  if (uri.scheme !== 'file') {
    throw new Error(`Unsupported scheme: ${uri.scheme}`)
  }
  const found = findTopLevelParent(uri.fsPath)
  if (!found) {
    throw new Error(`No baml_src directory found in path: ${uri.fsPath}`)
  }
  return found
}

class Project {
  private last_successful_runtime?: BamlWasm.WasmRuntime
  private current_runtime?: BamlWasm.WasmRuntime

  constructor(
    private files: BamlWasm.WasmProject,
    private ctx: BamlWasm.WasmRuntimeContext,
    private onSuccess: (e: WasmDiagnosticError, files: Record<string, string>) => void,
  ) {}

  update_runtime() {
    if (this.current_runtime == undefined) {
      try {
        this.current_runtime = this.files.runtime(this.ctx)

        const files = this.files.files()
        const fileMap = Object.fromEntries(
          files.map((f): [string, string] => f.split('BAML_PATH_SPLTTER', 2) as [string, string]),
        )
        this.onSuccess(this.files.diagnostics(this.current_runtime), fileMap)
      } catch (e) {
        console.error(`Error updating runtime: ${e}`)
        this.current_runtime = undefined
        throw e
      }
    }
  }

  requestDiagnostics() {
    if (this.current_runtime) {
      const files = this.files.files()
      const fileMap = Object.fromEntries(
        files.map((f): [string, string] => f.split('BAML_PATH_SPLTTER', 2) as [string, string]),
      )
      this.onSuccess(this.files.diagnostics(this.current_runtime), fileMap)
    }
  }

  runtime(): BamlWasm.WasmRuntime {
    const rt = this.current_runtime ?? this.last_successful_runtime
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

  get_file(file_path: string) {

    // Read the file content
    const fileContent = readFileSync(file_path, 'utf8');
  
    // Create a TextDocument
    const doc = TextDocument.create(file_path, 'plaintext', 1, fileContent);
  
    return doc;
  }

  upsert_file(file_path: string, content: string | undefined) {
    this.files.update_file(file_path, content)
    if (this.current_runtime) {
      this.last_successful_runtime = this.current_runtime
    }
    this.current_runtime?.free()
    this.current_runtime = undefined
  }

  handleDefinitionRequest(doc: TextDocument, position: Position): LocationLink[] {
      
    const word = getWordAtPosition(doc, position)
    
    //clean non-alphanumeric characters besides underscores and periods
    const cleaned_word = trimLine(word)
    if (cleaned_word === '') {
      
      return []
    }

    // Search for the symbol in the runtime
    const match = this.runtime().searchForSymbol(cleaned_word)

    // If we found a match, return the location
    if (match) {
      return [
        {
          targetUri: match.uri.toString(),
          //unused default values for now
          targetRange: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 0 }
          },
          targetSelectionRange: {
            start: { line: match.start_line, character: match.start_character },
            end: { line: match.end_line, character: match.end_character }
          }
        }
      ];
    }
    
    
    return [];
  }

  handleHoverRequest(doc: TextDocument, position: Position): Hover {
    const word = getWordAtPosition(doc, position)
    const cleaned_word = trimLine(word)
    if (cleaned_word === '') {
      return { contents: [] }
    }

    const match = this.runtime().search_for_symbol(cleaned_word)
    
    //need to get the content of the range specified by match's start and end lines and characters
    if (match) {
      const hoverCont: { language: string; value: string }[] = []

      const range = {
        start: { line: match.start_line, character: match.start_character },
        end: { line: match.end_line, character: match.end_character }
      }
      
      
      const hoverDoc = this.get_file(match.uri)

      if (hoverDoc) {


        const hoverText = hoverDoc.getText(range)
        
        hoverCont.push({ language: 'baml', value: hoverText })
      
        return {contents: hoverCont}
      }

    }

    return { contents: [] }
  }

  list_functions(): BamlWasm.WasmFunction[] {
    let runtime = this.runtime()

    return runtime.list_functions(this.ctx)
  }

  runGenerators(): BamlWasm.WasmGenerator[] {
    let runtime = this.runtime()

    return runtime.run_generators(this.ctx)
  }

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

  constructor(private notifier: Notify) {}

  private handleMessage(e: any) {
    if (e instanceof BamlWasm.WasmDiagnosticError) {
      const diagnostics = new Map<string, Diagnostic[]>(e.all_files.map((f) => [f, []]))

      e.errors().forEach((err) => {
        if (err.type === 'error') {
          console.error(`${err.message}, ${err.start_line}, ${err.start_column}, ${err.end_line}, ${err.end_column}`)
        }
        diagnostics.get(err.file_path)!.push({
          range: {
            start: {
              line: err.start_line,
              character: err.start_column,
            },
            end: {
              line: err.end_line,
              character: err.end_column,
            },
          },
          message: err.message,
          severity: err.type === 'error' ? DiagnosticSeverity.Error : DiagnosticSeverity.Warning,
          source: 'baml',
        })
      })
      this.notifier({ errors: Array.from(diagnostics), type: 'diagnostic' })
    } else if (e instanceof Error) {
      console.error('Error linting, got error ' + e.message)
      this.notifier({ message: e.message, type: 'error' })
    } else {
      console.error('Error linting ' + JSON.stringify(e))
      this.notifier({
        message: `${e}`,
        type: 'error',
      })
    }
  }

  private wrapSync<T>(fn: () => T): T | undefined {
    try {
      return fn()
    } catch (e) {
      this.handleMessage(e)
      return undefined
    }
  }

  private async wrapAsync<T>(fn: () => Promise<T>): Promise<T | undefined> {
    return await fn().catch((e) => {
      this.handleMessage(e)
      return undefined
    })
  }

  static version(): string {
    return BamlWasm.version()
  }

  private get_project(root_path: string) {
    const project = this.projects.get(root_path)
    if (!project) {
      throw new Error(`Project not found for path: ${root_path}`)
    }

    return project
  }

  private add_project(root_path: string, files: { [path: string]: string }) {
    console.debug(`Adding project: ${root_path}`)
    const project = BamlWasm.WasmProject.new(root_path, files)
    this.projects.set(
      root_path,
      new Project(project, new BamlWasm.WasmRuntimeContext(), (d, files) => {
        this.handleMessage(d)
        this.notifier({ type: 'runtime_updated', root_path, files })
      }),
    )
    return this.get_project(root_path)!
  }

  private remove_project(root_path: string) {
    this.projects.delete(root_path)
  }

  async upsert_file(path: URI, content: string | undefined) {
    console.debug(
      `Upserting file: ${path}. Current projects ${this.projects.size} ${JSON.stringify(this.projects, null, 2)}`,
    )
    await this.wrapAsync(async () => {
      console.debug(
        `Upserting file: ${path}. current projects  ${this.projects.size}  ${JSON.stringify(this.projects, null, 2)}`,
      )
      const rootPath = uriToRootPath(path)
      if (this.projects.has(rootPath)) {
        const project = this.get_project(rootPath)
        project.upsert_file(path.fsPath, content)
        project.update_runtime()
      } else {
        await this.reload_project_files(path)
      }
    })
  }

  async save_file(path: URI, content: string) {
    console.debug(`Saving file: ${path}`)
    await this.wrapAsync(async () => {
      const rootPath = uriToRootPath(path)
      if (this.projects.has(rootPath)) {
        const project = this.get_project(rootPath)
        project.save_file(path.fsPath, content)
        project.update_runtime()
      } else {
        await this.reload_project_files(path)
      }
    })
  }

  update_unsaved_file(path: URI, content: string) {
    console.debug(`Updating unsaved file: ${path}`)
    this.wrapSync(() => {
      const rootPath = uriToRootPath(path)
      const project = this.get_project(rootPath)
      project.update_unsaved_file(path.fsPath, content)
      project.update_runtime()
    })
  }

  async touch_project(path: URI) {
    await this.wrapAsync(async () => {
      const rootPath = uriToRootPath(path)
      if (!this.projects.has(rootPath)) {
        await this.reload_project_files(path)
      }
    })
  }

  async requestDiagnostics() {
    console.debug('Requesting diagnostics')
    await this.wrapAsync(async () => {
      for (const project of this.projects.values()) {
        project.requestDiagnostics()
      }
    })
  }

  // Reload all files in a project
  // Takes in a URI to any file in the project
  async reload_project_files(path: URI) {
    console.debug(`Reloading project files: ${path}`)
    await this.wrapAsync(async () => {
      const rootPath = uriToRootPath(path)

      const files = await Promise.all(
        gatherFiles(rootPath).map(async (uri): Promise<[string, string]> => {
          const path = uri.fsPath
          const content = await readFile(path, 'utf8')
          return [path, content]
        }),
      )

      if (files.length === 0) {
        this.notifier({
          type: 'warn',
          message: `Empty baml_src directory found: ${rootPath}. See Output panel -> BAML Language Server for more details.`,
        })
      }
      console.debug(`projects ${this.projects.size}: ${JSON.stringify(this.projects, null, 2)},`)
      console.info(this.projects)

      if (!this.projects.has(rootPath)) {
        const project = this.add_project(rootPath, Object.fromEntries(files))
        project.update_runtime()
      } else {
        const project = this.get_project(rootPath)

        project.replace_all_files(BamlWasm.WasmProject.new(rootPath, Object.fromEntries(files)))
        project.update_runtime()
      }
    })
  }

  getProjectById(id: URI): Project {
    return this.get_project(uriToRootPath(id));
  }


  runGenerators() {
    for (const project of this.projects.values()) {
      const files = project.runGenerators()
      for (const f of files) {
        console.log(f.path, f.contents.length)
      }
    }
  }

  
}

export default BamlProjectManager
