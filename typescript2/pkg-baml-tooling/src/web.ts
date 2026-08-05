import { request, WasmBackend } from './backend.js';
import type {
  CheckResult,
  ProjectLayout,
  SegmentMap,
  VirtualModule,
} from './generated/tooling.js';

export interface WebLoadProjectOptions {
  cwd: string;
  files: Record<string, string>;
  target?: 'node' | 'web';
  toolingRequest(request: Uint8Array): Uint8Array;
}

/** Browser-safe project facade. All filesystem snapshots are caller-owned. */
export class WebBamlProject {
  readonly #backend: WasmBackend;
  readonly #projectId: string;
  #fingerprint: string;
  #layout: ProjectLayout;
  #version = 0;
  private constructor(
    backend: WasmBackend,
    projectId: string,
    fingerprint: string,
    layout: ProjectLayout,
  ) {
    this.#backend = backend;
    this.#projectId = projectId;
    this.#fingerprint = fingerprint;
    this.#layout = layout;
  }

  static async open(options: WebLoadProjectOptions): Promise<WebBamlProject> {
    const backend = new WasmBackend(options.toolingRequest);
    const opened = request(backend, {
      request: {
        $case: 'open',
        open: {
          files: Object.entries(options.files).map(([path, text]) => ({
            path,
            text,
          })),
          projectRoot: options.cwd,
          target: options.target ?? 'web',
        },
      },
    });
    if (opened.response?.$case !== 'project')
      throw new Error('BAML WASM bridge did not open the project');
    const projectId = opened.response.project.projectId;
    const layout = request(backend, {
      request: { $case: 'layout', layout: { projectId } },
    });
    if (layout.response?.$case !== 'layout')
      throw new Error('BAML WASM bridge omitted project layout');
    return new WebBamlProject(
      backend,
      projectId,
      opened.response.project.fingerprint,
      layout.response.layout,
    );
  }

  layout(): ProjectLayout {
    return this.#layout;
  }
  fingerprint(): string {
    return this.#fingerprint;
  }
  check(): CheckResult {
    const response = request(this.#backend, {
      request: { $case: 'check', check: { projectId: this.#projectId } },
    });
    if (response.response?.$case !== 'check')
      throw new Error('BAML WASM bridge returned an invalid check response');
    return response.response.check;
  }
  updateFile(path: string, text: string | null): void {
    this.#version += 1;
    const response = request(this.#backend, {
      request: {
        $case: 'update',
        update: {
          file: { path, text: text ?? '' },
          projectId: this.#projectId,
          remove: text === null,
          version: this.#version,
        },
      },
    });
    if (response.response?.$case !== 'project')
      throw new Error('BAML WASM bridge rejected the update');
    this.#fingerprint = response.response.project.fingerprint;
  }
  resolveModule(
    id: string,
    importer = `${this.#layout.roots[0] ?? ''}/index.ts`,
  ): { code: string; watchFiles: string[] } {
    const module = this.#module(id, importer);
    return { code: module.code, watchFiles: module.watchFiles };
  }
  resolveDts(
    id: string,
    importer = `${this.#layout.roots[0] ?? ''}/index.ts`,
  ): { code: string; map: SegmentMap; watchFiles: string[]; stale: boolean } {
    const module = this.#module(id, importer);
    if (!module.map) throw new Error('BAML WASM bridge omitted a segment map');
    return {
      code: module.declaration,
      map: module.map,
      stale: module.stale,
      watchFiles: module.watchFiles,
    };
  }
  dispose(): void {
    // The WASM instance is caller-owned; dropping this facade releases its state.
  }
  #module(id: string, importer: string): VirtualModule {
    const response = request(this.#backend, {
      request: {
        $case: 'module',
        module: { importer, projectId: this.#projectId, specifier: id },
      },
    });
    if (response.response?.$case !== 'module')
      throw new Error(`BAML WASM bridge could not resolve ${id}`);
    return response.response.module;
  }
}

export async function loadProject(
  options: WebLoadProjectOptions,
): Promise<WebBamlProject> {
  return WebBamlProject.open(options);
}
