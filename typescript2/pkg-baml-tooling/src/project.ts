import { createHash } from 'node:crypto';
import {
  type Dirent,
  existsSync,
  readdirSync,
  readFileSync,
  realpathSync,
  statSync,
} from 'node:fs';
import { createRequire } from 'node:module';
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
  /**
   * Lists, recursively, every `.baml` path the host can see under
   * `directory`. Hosts whose files never reach the process filesystem — an
   * editor overlay for an unsaved buffer, a WASM or in-memory host — supply
   * this so file-set discovery is not silently reduced to `readdirSync`.
   * Returning `undefined` (or omitting it) falls back to the disk walk.
   */
  readDirectory?(directory: string): readonly string[] | undefined;
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
  #disposed = false;

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
    const files = discoverBamlFiles(root, options);
    const readFile =
      options.readFile ?? ((path: string) => readFileSync(path, 'utf8'));
    const configPath = canonical(join(root, 'baml.toml'));
    const configText = readFile(configPath);
    if (configText !== undefined) files.set(configPath, configText);
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

  refreshLayout(): ProjectLayout {
    const response = request(this.#backend, {
      request: { $case: 'layout', layout: { projectId: this.#projectId } },
    });
    if (response.response?.$case !== 'layout')
      throw new Error('BAML bridge returned an invalid layout response');
    this.#layout = response.response.layout;
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
    if (id.startsWith('\0baml:') && id.endsWith(':runtime')) {
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
  /**
   * Releases the compiler session behind this project. The bridge drops the
   * session's database and every allocation the host reached through it, on
   * the native and the WASM host alike — both route the same `close` request
   * into the same protocol, so neither can drift into leaking what the other
   * frees. Without this the superseded session of every config-driven lane
   * replacement would survive for the life of the process.
   *
   * Idempotent, and never throws: disposal runs on teardown paths where a
   * bridge that has already dropped the session (or died outright) has
   * nothing left to release. Using the project afterwards fails loudly with
   * `unknown_project` rather than silently reopening a session.
   */
  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    try {
      request(this.#backend, {
        request: { $case: 'close', close: { projectId: this.#projectId } },
      });
    } catch {
      // Already gone, or the bridge is unusable; either way there is nothing
      // to release and a teardown path must not fail.
    }
    // The bridge owns the compiler database, but these mirrors are the host's
    // own copy of every source and emitted module and would otherwise pin a
    // superseded lane's memory for as long as something held the project.
    this.#texts.clear();
    this.#modules.clear();
    this.#versions.clear();
    this.#runtime = undefined;
    this.#backend.dispose?.();
  }

  /** Whether `dispose()` has released this project's session. */
  get disposed(): boolean {
    return this.#disposed;
  }

  #module(id: string, importer: string): VirtualModule {
    const key = this.#moduleKey(id, importer);
    const cached = this.#modules.get(key);
    if (cached) return cached;
    // The bridge cannot run Node package resolution: a bare specifier joined
    // onto the importer's directory would target a nonexistent path and the
    // session would (correctly) refuse it. Resolve to the physical absolute
    // path here, where node_modules and pnpm symlinks are visible.
    const specifier =
      id !== 'baml:client' && id.endsWith('.baml')
        ? (resolveBamlSpecifier(id, importer) ??
          (() => {
            throw new Error(
              `Could not resolve BAML import '${id}' from ${importer} through Node module resolution`,
            );
          })())
        : id;
    const response = request(this.#backend, {
      request: {
        $case: 'module',
        module: { importer, projectId: this.#projectId, specifier },
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
      return (
        resolveBamlSpecifier(id, importer) ?? `${id}\0${canonical(importer)}`
      );
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
  access: FileAccess = {},
): Map<string, string> {
  const readFile =
    access.readFile ?? ((path: string) => readFileSync(path, 'utf8'));
  const files = new Map<string, string>();
  const add = (path: string) => {
    const text = readFile(path);
    if (text !== undefined) files.set(canonical(path), text);
  };
  walkBamlFiles(root, add);
  // Host-visible sources the disk walk cannot see (unsaved editor buffers,
  // WASM/in-memory hosts) still belong to the project.
  for (const path of hostListing(access, root)) add(path);
  return files;
}

/**
 * Recursive `.baml` walk over the process filesystem, shared by open-time
 * discovery and the incremental poller. A directory that cannot be read is
 * not an error: a host with no process filesystem simply has nothing there,
 * and contributes its files through `FileAccess.readDirectory` instead.
 */
function walkBamlFiles(
  root: string,
  onFile: (path: string) => void,
  onDirectory: (directory: string) => void = () => undefined,
): void {
  const visit = (directory: string) => {
    onDirectory(directory);
    let entries: Dirent[] = [];
    try {
      entries = readdirSync(directory, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (
        entry.name === 'node_modules' ||
        entry.name === '.git' ||
        entry.name === '.baml'
      )
        continue;
      const path = join(directory, entry.name);
      if (entry.isDirectory()) visit(path);
      else if (entry.name.endsWith('.baml') && !isBamlSidecar(entry.name))
        onFile(path);
    }
  };
  visit(root);
}

/** The host's own recursive `.baml` listing, filtered the same way the disk
 * walk filters: sources only, never sidecars. */
function hostListing(access: FileAccess, root: string): string[] {
  return (access.readDirectory?.(root) ?? []).filter(
    (path) => path.endsWith('.baml') && !isBamlSidecar(path),
  );
}

/**
 * Resolves a `.baml` import specifier to its physical absolute path.
 * Relative specifiers join onto the importer; bare specifiers
 * (`dep/baml_src/widget.baml`) go through Node's module algorithm from the
 * importer, so real installs and pnpm-style symlinked node_modules entries
 * both land on the physical file. Returns `undefined` when a bare specifier
 * cannot be resolved (missing package, or a package `exports` map that does
 * not expose the subpath).
 */
export function resolveBamlSpecifier(
  specifier: string,
  importer: string,
): string | undefined {
  if (specifier === 'baml:client') return specifier;
  if (isAbsolute(specifier)) return canonical(specifier);
  if (specifier.startsWith('.'))
    return canonical(resolve(dirname(importer), specifier));
  try {
    return canonical(createRequire(importer).resolve(specifier));
  } catch {
    return undefined;
  }
}

/**
 * Incremental `.baml` file-set discovery for long-lived editor sessions.
 * A full recursive walk on every language-service request scales with the
 * project size and is paid per keystroke, so `poll()` re-walks only when a
 * known directory's mtime stamp moved (a file or subdirectory added or
 * removed anywhere in the tree changes its immediate parent's stamp, and
 * every parent chain from the root is tracked). Content changes to existing
 * files do not move directory stamps; hosts track those through per-file
 * version counters instead.
 *
 * Discovery is host-scoped, never process-filesystem-scoped. A host whose
 * files never land on disk — an unsaved editor buffer, a WASM or in-memory
 * host — reaches the file set through two channels that the disk walk cannot
 * provide: `FileAccess.readDirectory`, compared on every poll, and `track()`,
 * which registers an individual import target. Without them a host-only
 * source would stay undiscoverable until someone wrote it to disk.
 */
export class BamlFileDiscovery {
  readonly #root: string;
  readonly #canonicalRoot: string;
  readonly #host: FileAccess;
  readonly #tracked = new Set<string>();
  #files = new Set<string>();
  #stamps = new Map<string, number>();
  #listed = new Set<string>();
  #primed = false;

  constructor(root: string, host: FileAccess = {}) {
    this.#root = root;
    this.#canonicalRoot = canonical(root);
    this.#host = host;
  }

  /**
   * Registers a `.baml` path the host resolved — an import target — as a
   * discovery candidate. No filesystem walk can see a host-only overlay, so
   * this is how an unsaved buffer that something already imports joins the
   * compiler's file set. Paths outside the discovery root, sidecars, and
   * non-`.baml` paths are ignored; existence is re-checked through the host
   * on every poll, so a candidate that never materializes is never reported.
   */
  track(path: string): void {
    const file = canonical(path);
    if (!file.endsWith('.baml') || isBamlSidecar(file)) return;
    if (!withinRoot(this.#canonicalRoot, file)) return;
    this.#tracked.add(file);
  }

  /** Returns the current `.baml` file set after a re-walk, or `undefined`
   * when no directory stamp, host listing entry, or tracked candidate moved
   * since the previous poll (no re-walk). */
  poll(): Set<string> | undefined {
    const tracked = [...this.#tracked].filter((path) => this.#exists(path));
    const listed = new Set(
      hostListing(this.#host, this.#root).map((path) => canonical(path)),
    );
    if (this.#primed && !this.#dirty(tracked, listed)) return undefined;
    const files = new Set<string>();
    const stamps = new Map<string, number>();
    walkBamlFiles(
      this.#root,
      (path) => files.add(canonical(path)),
      (directory) => stamps.set(directory, directoryStamp(directory)),
    );
    for (const path of listed) files.add(path);
    for (const path of tracked) files.add(path);
    this.#stamps = stamps;
    this.#listed = listed;
    this.#files = files;
    this.#primed = true;
    return files;
  }

  /**
   * A poll is dirty when a tracked candidate joined the set, when the host's
   * own listing changed in either direction, or when a tracked directory's
   * stamp moved. The listing is compared on every poll rather than only
   * inside a re-walk: a host-only buffer that nothing imports yet moves no
   * directory stamp and registers no import target, so gating the listing
   * behind the stamp check would strand it — the compiler layout, the
   * `baml:client` declarations, diagnostics, and completions would all stay
   * stale until the file was imported, saved, or some unrelated filesystem
   * change happened to force a walk.
   */
  #dirty(tracked: string[], listed: Set<string>): boolean {
    if (tracked.some((path) => !this.#files.has(path))) return true;
    if (listed.size !== this.#listed.size) return true;
    for (const path of listed) if (!this.#listed.has(path)) return true;
    return this.#moved();
  }

  #exists(path: string): boolean {
    return this.#host.fileExists?.(path) ?? existsSync(path);
  }

  #moved(): boolean {
    for (const [directory, stamp] of this.#stamps)
      if (stampMoved(stamp, directoryStamp(directory))) return true;
    return false;
  }
}

function directoryStamp(directory: string): number {
  try {
    return statSync(directory).mtimeMs;
  } catch {
    return Number.NaN;
  }
}

/** A directory that is absent on both polls has not moved. `NaN !== NaN`
 * would otherwise report every in-memory host's root as dirty forever,
 * re-walking on every keystroke — the exact cost stamping exists to avoid. */
function stampMoved(previous: number, current: number): boolean {
  if (Number.isNaN(previous) && Number.isNaN(current)) return false;
  return previous !== current;
}

function withinRoot(root: string, path: string): boolean {
  const inside = relative(root, path);
  return inside !== '' && !inside.startsWith('..') && !isAbsolute(inside);
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
