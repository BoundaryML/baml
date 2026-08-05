import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, realpathSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve } from 'node:path';
import type { ToolingBackend } from './backend.js';
import { request } from './backend.js';
import { DiskArtifactCache } from './cache.js';
import type {
  Capabilities,
  CheckResult,
  CompletionItem,
  Hover,
  Location,
  ProjectLayout,
  RuntimeModule,
  SegmentMap,
  VirtualModule,
  WorkspaceEdit,
} from './generated/tooling.js';
import { generatedToSource, sourceToGenerated } from './mapping.js';
import {
  RENAME_CAPABILITY,
  TOOLING_PROTOCOL_VERSION,
  TYPESCRIPT_IMPORTS_CAPABILITY,
} from './protocol.js';
import { isBamlSidecar } from './sidecar.js';

export interface FileAccess {
  readFile?(path: string): string | undefined;
  fileExists?(path: string): boolean;
}

export interface OpenProjectOptions extends FileAccess {
  cwd: string;
  cacheDir?: string | false;
  target?: 'node' | 'web';
  backend: ToolingBackend;
  root?: string;
}

export class BamlProject {
  readonly #backend: ToolingBackend;
  readonly #projectId: string;
  readonly #root: string;
  readonly #target: 'node' | 'web';
  readonly #texts = new Map<string, string>();
  readonly #versions = new Map<string, number>();
  readonly #modules = new Map<string, VirtualModule>();
  #cache?: DiskArtifactCache;
  #cachePrefix = '';
  #runtime?: RuntimeModule;
  #layout: ProjectLayout;
  #capabilities: Capabilities;
  #fingerprint: string;

  private constructor(
    options: OpenProjectOptions,
    state: { projectId: string; fingerprint: string },
    layout: ProjectLayout,
    capabilities: Capabilities,
    files: Map<string, string>,
  ) {
    this.#backend = options.backend;
    this.#projectId = state.projectId;
    this.#fingerprint = state.fingerprint;
    this.#root = options.root ?? options.cwd;
    this.#target = options.target ?? 'node';
    this.#layout = layout;
    this.#capabilities = capabilities;
    this.#texts = files;
  }

  static async open(options: OpenProjectOptions): Promise<BamlProject> {
    const root = discoverProject(
      options.root ?? options.cwd,
      options.fileExists,
    );
    const files = discoverBamlFiles(root, options.readFile);
    const response = request(options.backend, {
      request: {
        $case: 'open',
        open: {
          files: [...files].map(([path, text]) => ({ path, text })),
          projectRoot: root,
          target: options.target ?? 'node',
        },
      },
    });
    if (response.response?.$case !== 'project')
      throw new Error('BAML bridge did not open the project');
    const openedProjectId = response.response.project.projectId;
    const layoutResponse = request(options.backend, {
      request: { $case: 'layout', layout: { projectId: openedProjectId } },
    });
    const capabilitiesResponse = request(options.backend, {
      request: {
        $case: 'capabilities',
        capabilities: { projectId: openedProjectId },
      },
    });
    if (
      layoutResponse.response?.$case !== 'layout' ||
      capabilitiesResponse.response?.$case !== 'capabilities'
    )
      throw new Error('BAML bridge handshake failed');
    const capabilities = capabilitiesResponse.response.capabilities;
    if (
      capabilities.protocol !== TOOLING_PROTOCOL_VERSION ||
      !capabilities.features.includes(TYPESCRIPT_IMPORTS_CAPABILITY)
    ) {
      throw new Error(
        `BAML tooling protocol skew: expected ${TOOLING_PROTOCOL_VERSION}; update @boundaryml/baml-tooling and @boundaryml/baml-bridge-tooling together`,
      );
    }
    const project = new BamlProject(
      { ...options, root },
      response.response.project,
      layoutResponse.response.layout,
      capabilities,
      files,
    );
    if (options.backend.kind === 'native' && options.cacheDir !== false) {
      const base = options.cacheDir ?? join(root, '.baml/cache/ts-tooling/v1');
      project.#cache = new DiskArtifactCache(join(base, projectId(root)));
      project.#refreshCachePrefix();
      await project.#hydrateCache();
    }
    return project;
  }

  layout(): ProjectLayout {
    return this.#layout;
  }
  capabilities(): Capabilities {
    return this.#capabilities;
  }
  fingerprint(): string {
    return this.#fingerprint;
  }

  updateFile(path: string, text: string | null, version?: number): void {
    const canonicalPath = canonical(path);
    const nextVersion = version ?? (this.#versions.get(canonicalPath) ?? 0) + 1;
    const response = request(this.#backend, {
      request: {
        $case: 'update',
        update: {
          file: { path: canonicalPath, text: text ?? '' },
          projectId: this.#projectId,
          remove: text === null,
          version: nextVersion,
        },
      },
    });
    if (response.response?.$case !== 'project')
      throw new Error('BAML bridge rejected the update');
    this.#versions.set(canonicalPath, nextVersion);
    if (text === null) this.#texts.delete(canonicalPath);
    else this.#texts.set(canonicalPath, text);
    this.#fingerprint = response.response.project.fingerprint;
    this.#modules.clear();
    this.#runtime = undefined;
    this.#refreshCachePrefix();
  }

  check(): CheckResult {
    const response = request(this.#backend, {
      request: { $case: 'check', check: { projectId: this.#projectId } },
    });
    if (response.response?.$case !== 'check')
      throw new Error('BAML bridge returned an invalid check response');
    return response.response.check;
  }

  resolveModule(
    id: string,
    importer = join(this.#root, 'index.ts'),
  ): { code: string; watchFiles: string[] } {
    if (id.startsWith('\\0baml:') && id.endsWith(':runtime')) {
      const runtime = this.resolveRuntime();
      return { code: runtime.code, watchFiles: runtime.watchFiles };
    }
    const module = this.#module(id, importer);
    return { code: module.code, watchFiles: module.watchFiles };
  }

  resolveDts(
    id: string,
    importer = join(this.#root, 'index.ts'),
  ): { code: string; map: SegmentMap; watchFiles: string[]; stale: boolean } {
    const module = this.#module(id, importer);
    if (!module.map)
      throw new Error(`BAML bridge omitted the segment map for ${id}`);
    const hashesMatch = module.map.sources.every((source, index) => {
      const text = this.#texts.get(canonical(source));
      return (
        text !== undefined &&
        createHash('sha256').update(text).digest('hex') ===
          module.map?.sourceHashes[index]
      );
    });
    const map = hashesMatch ? module.map : { ...module.map, segments: [] };
    return {
      code: module.declaration,
      map,
      stale: module.stale || !hashesMatch,
      watchFiles: module.watchFiles,
    };
  }

  resolveRuntime(): RuntimeModule {
    if (this.#runtime) return this.#runtime;
    const response = request(this.#backend, {
      request: {
        $case: 'runtimeModule',
        runtimeModule: { projectId: this.#projectId },
      },
    });
    if (response.response?.$case !== 'runtimeModule')
      throw new Error('BAML bridge returned an invalid runtime module');
    this.#runtime = response.response.runtimeModule;
    this.#persist('runtime', this.#runtime);
    return this.#runtime;
  }

  mappingsFor(id: string, importer?: string): SegmentMap {
    return this.resolveDts(id, importer).map;
  }

  definitionAt(path: string, offsetUtf8: number, symbolId = ''): Location[] {
    return this.#locations('definition', path, offsetUtf8, symbolId);
  }
  referencesAt(path: string, offsetUtf8: number, symbolId = ''): Location[] {
    return this.#locations('references', path, offsetUtf8, symbolId);
  }

  hoverAt(path: string, offsetUtf8: number, symbolId: string): Hover {
    const response = request(this.#backend, {
      request: {
        $case: 'hover',
        hover: { offsetUtf8, path, projectId: this.#projectId, symbolId },
      },
    });
    if (response.response?.$case !== 'hover')
      throw new Error('BAML bridge returned invalid hover data');
    return response.response.hover;
  }

  completionsAt(
    path = '',
    offsetUtf8 = 0,
    entry = 'baml:client',
  ): CompletionItem[] {
    const response = request(this.#backend, {
      request: {
        $case: 'completions',
        completions: {
          offsetUtf8,
          path,
          projectId: this.#projectId,
          symbolId: entry,
        },
      },
    });
    if (response.response?.$case !== 'completions')
      throw new Error('BAML bridge returned invalid completions');
    return response.response.completions.items;
  }

  prepareRename(symbolId: string): Location {
    if (!this.#capabilities.features.includes(RENAME_CAPABILITY))
      throw new Error(
        'rename across the BAML boundary requires a newer BAML compiler bridge',
      );
    const response = request(this.#backend, {
      request: {
        $case: 'prepareRename',
        prepareRename: {
          offsetUtf8: 0,
          path: '',
          projectId: this.#projectId,
          symbolId,
        },
      },
    });
    if (response.response?.$case !== 'renameCheck')
      throw new Error('BAML bridge refused rename');
    return response.response.renameCheck;
  }

  rename(symbolId: string, newName: string): WorkspaceEdit {
    this.prepareRename(symbolId);
    const response = request(this.#backend, {
      request: {
        $case: 'rename',
        rename: {
          newName,
          offsetUtf8: 0,
          path: '',
          projectId: this.#projectId,
          symbolId,
        },
      },
    });
    if (response.response?.$case !== 'rename')
      throw new Error('BAML bridge returned invalid rename edits');
    return response.response.rename;
  }

  generatedToSource(id: string, offsetUtf16: number, importer?: string) {
    return generatedToSource(this.mappingsFor(id, importer), offsetUtf16);
  }
  sourceToGenerated(
    id: string,
    path: string,
    offsetUtf8: number,
    importer?: string,
  ) {
    return sourceToGenerated(this.mappingsFor(id, importer), path, offsetUtf8);
  }
  sourceText(path: string): string | undefined {
    return this.#texts.get(canonical(path));
  }
  dispose(): void {
    this.#backend.dispose?.();
  }

  #module(id: string, importer: string): VirtualModule {
    const key = this.#moduleKey(id, importer);
    const cached = this.#modules.get(key);
    if (cached) return cached;
    const response = request(this.#backend, {
      request: {
        $case: 'module',
        module: { importer, projectId: this.#projectId, specifier: id },
      },
    });
    if (response.response?.$case !== 'module')
      throw new Error(`BAML bridge could not resolve ${id}`);
    this.#modules.set(key, response.response.module);
    this.#persist(`module\0${key}`, response.response.module);
    return response.response.module;
  }

  #moduleKey(id: string, importer: string): string {
    if (id === 'baml:client') return id;
    if (id.endsWith('.baml'))
      return canonical(isAbsolute(id) ? id : resolve(dirname(importer), id));
    return `${id}\0${canonical(importer)}`;
  }

  #refreshCachePrefix(): void {
    if (!this.#cache) return;
    const sources = [...this.#texts]
      .sort(([left], [right]) => left.localeCompare(right))
      .flat();
    this.#cachePrefix = this.#cache.key([
      TOOLING_PROTOCOL_VERSION,
      this.#fingerprint,
      this.#target,
      ...sources,
    ]);
  }

  async #hydrateCache(): Promise<void> {
    if (!this.#cache) return;
    const moduleKeys = [
      'baml:client',
      ...this.#layout.sourceFiles.map(canonical),
    ];
    await Promise.all([
      ...moduleKeys.map(async (key) => {
        const module = await this.#cache?.singleFlight(
          this.#artifactKey(`module\0${key}`),
          () =>
            this.#cache?.get<VirtualModule>(
              this.#artifactKey(`module\0${key}`),
            ) ?? Promise.resolve(undefined),
        );
        if (module && this.#validModule(module)) this.#modules.set(key, module);
      }),
      (async () => {
        const runtime = await this.#cache?.singleFlight(
          this.#artifactKey('runtime'),
          () =>
            this.#cache?.get<RuntimeModule>(this.#artifactKey('runtime')) ??
            Promise.resolve(undefined),
        );
        if (runtime?.fingerprint === this.#fingerprint) this.#runtime = runtime;
      })(),
    ]);
  }

  #validModule(module: VirtualModule): boolean {
    if (module.fingerprint !== this.#fingerprint || !module.map) return false;
    return module.map.sources.every((source, index) => {
      const text = this.#texts.get(canonical(source));
      return (
        text !== undefined &&
        createHash('sha256').update(text).digest('hex') ===
          module.map?.sourceHashes[index]
      );
    });
  }

  #artifactKey(kind: string): string {
    return this.#cache?.key([this.#cachePrefix, kind]) ?? '';
  }

  #persist(kind: string, value: VirtualModule | RuntimeModule): void {
    if (!this.#cache) return;
    void this.#cache.put(this.#artifactKey(kind), value).catch(() => undefined);
  }

  #locations(
    kind: 'definition' | 'references',
    path: string,
    offsetUtf8: number,
    symbolId: string,
  ): Location[] {
    const value = { offsetUtf8, path, projectId: this.#projectId, symbolId };
    const response = request(this.#backend, {
      request:
        kind === 'definition'
          ? { $case: 'definition', definition: value }
          : { $case: 'references', references: value },
    });
    if (response.response?.$case !== 'locations')
      throw new Error(`BAML bridge returned invalid ${kind} locations`);
    return response.response.locations.locations;
  }
}

export function discoverProject(
  start: string,
  fileExists: FileAccess['fileExists'] = existsSync,
): string {
  let current = resolve(start);
  while (true) {
    if (fileExists?.(join(current, 'baml.toml'))) return canonical(current);
    const parent = dirname(current);
    if (parent === current)
      throw new Error(`No baml.toml found above ${start}`);
    current = parent;
  }
}

export function discoverBamlFiles(
  root: string,
  readFile: FileAccess['readFile'] = (path) => readFileSync(path, 'utf8'),
): Map<string, string> {
  const files = new Map<string, string>();
  const visit = (directory: string) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      if (
        entry.name === 'node_modules' ||
        entry.name === '.git' ||
        entry.name === '.baml'
      )
        continue;
      const path = join(directory, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.name.endsWith('.baml') && !isBamlSidecar(entry.name)) {
        const text = readFile?.(path);
        if (text !== undefined) files.set(canonical(path), text);
      }
    }
  };
  visit(root);
  return files;
}

export function projectId(root: string): string {
  return createHash('sha256')
    .update(canonical(root))
    .digest('hex')
    .slice(0, 16);
}
export function moduleSpecifier(source: string, importer: string): string {
  return isAbsolute(source)
    ? source
    : `./${relative(dirname(importer), source).replaceAll('\\', '/')}`;
}
function canonical(path: string): string {
  try {
    return realpathSync(path);
  } catch {
    return resolve(path);
  }
}
