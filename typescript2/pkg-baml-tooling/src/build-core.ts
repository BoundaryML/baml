import { createHash } from 'node:crypto';
import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { createRequire } from 'node:module';
import { basename, dirname, isAbsolute, resolve, sep } from 'node:path';
import type { LoadProjectOptions } from './node.js';
import { loadProject } from './node.js';
import { type BamlProject, discoverProject, projectId } from './project.js';

export interface BamlPluginOptions {
  root?: string;
  include?: RegExp;
  exclude?: RegExp;
  target?: 'node' | 'web';
  cacheDir?: string | false;
  debug?: boolean;
}

export interface BuildHost {
  addWatchFile?(path: string): void;
  invalidate?(id: string): void;
  fullReload?(): void;
  development?: boolean;
  watchError?(error: unknown): void;
}

export type ProjectFactory = (
  options: LoadProjectOptions,
) => Promise<BamlProject>;

interface ResolvedImport {
  specifier: string;
  importer: string;
  root: string;
}

export class BamlBuildCore {
  readonly #options: BamlPluginOptions;
  readonly #factory: ProjectFactory;
  readonly #projects = new Map<string, BamlProject>();
  readonly #opening = new Map<string, Promise<string>>();
  readonly #projectIds = new Map<string, string>();
  readonly #idRoots = new Map<string, string>();
  readonly #resolved = new Map<string, ResolvedImport>();
  readonly #hashes = new Map<string, string>();
  readonly #declarationHashes = new Map<string, string>();
  readonly #lastGood = new Map<
    string,
    { code: string; watchFiles: string[] }
  >();
  readonly #watchFiles = new Map<string, string[]>();
  #primaryRoot = '';
  #host: BuildHost = {};
  #development = false;
  #pendingShapeChange = false;

  constructor(
    options: BamlPluginOptions = {},
    factory: ProjectFactory = loadProject,
  ) {
    this.#options = options;
    this.#factory = factory;
  }

  async start(host: BuildHost = {}): Promise<void> {
    this.#host = host;
    this.#development = host.development ?? this.#development;
    await this.#openProject(this.#options.root ?? process.cwd(), true);
  }

  enableDevelopment(): void {
    this.#development = true;
  }

  async resolve(
    id: string,
    importer = resolve(this.#options.root ?? process.cwd(), 'index.ts'),
  ): Promise<string | undefined> {
    if (this.#projects.size === 0)
      throw new Error('BAML build core has not started');
    if (id.startsWith('\0baml:')) return id;
    if (id !== 'baml:client' && !id.endsWith('.baml')) return undefined;
    if (this.#options.include && !this.#options.include.test(id))
      return undefined;
    if (this.#options.exclude?.test(id)) return undefined;
    if (id === 'baml:client') {
      const root = this.#rootForImporter(importer);
      const virtual = `\0baml:${this.#projectIds.get(root)}:file:client`;
      this.#resolved.set(virtual, { importer, root, specifier: id });
      return virtual;
    }
    const specifier = this.#resolveSpecifier(id, importer);
    if (!specifier) return undefined;
    // Sidecar precedence, bundler edition. Only `<source>.baml.ts` carries
    // runtime code, and it is resolved here explicitly rather than deferred:
    // returning undefined makes the bundler resolve `./main.baml` to the
    // literal `.baml` file — extension appending never fires for a path that
    // already exists — so it would never reach the sidecar at all.
    // `<source>.baml.d.ts` (everything `baml-ts-gen` emits, so that plain
    // `tsc --noEmit` works) carries declarations only; deferring for it would
    // hand every bundler the raw `.baml` source and fail the build in the JS
    // parser. Declarations never override the bundler's runtime, so the
    // module is virtualized exactly as if no sidecar existed.
    const runtimeSidecar = `${specifier}.ts`;
    if (existsSync(runtimeSidecar)) return canonical(runtimeSidecar);
    const root = await this.#owningRoot(specifier);
    const project = this.#projects.get(root);
    if (!project) return undefined;
    const canonicalSpecifier = canonical(specifier);
    // A `.baml` the owning session has not seen (created after open, e.g.
    // during a dev server session) is registered eagerly so the compiler
    // database never serves an empty or poisoned module for it.
    if (
      !project.layout().sourceFiles.includes(canonicalSpecifier) &&
      existsSync(specifier)
    ) {
      project.updateFile(specifier, readFileSync(specifier, 'utf8'));
      project.refreshLayout();
      this.#registerWatch(root, project);
    }
    const token = Buffer.from(canonicalSpecifier).toString('base64url');
    const virtual = `\0baml:${this.#projectIds.get(root)}:file:${token}`;
    this.#resolved.set(virtual, { importer, root, specifier });
    return virtual;
  }

  load(id: string): { code: string; watchFiles: string[] } | undefined {
    if (this.#projects.size === 0)
      throw new Error('BAML build core has not started');
    const runtime = /^\0baml:([0-9a-f]+):runtime$/.exec(id);
    if (runtime) {
      const root = this.#idRoots.get(runtime[1] ?? '');
      const project = root && this.#projects.get(root);
      if (!project) return undefined;
      const result = project.resolveModule(id);
      result.watchFiles = union(result.watchFiles, this.#watchFiles.get(root));
      this.#remember(id, result);
      return result;
    }
    const resolved = this.#resolved.get(id);
    if (!resolved) return undefined;
    const project = this.#projects.get(resolved.root);
    if (!project) return undefined;
    const check = project.check();
    const errors = check.diagnostics.filter(
      (diagnostic) => diagnostic.severity === 'error',
    );
    if (errors.length > 0) {
      const previous = this.#lastGood.get(id);
      if (this.#development && previous) return previous;
      const first = errors[0];
      const error = new Error(first?.message ?? 'BAML compilation failed');
      Object.assign(error, {
        diagnostics: errors,
        id: first?.location?.path,
        pos: first?.location?.startUtf8,
      });
      throw error;
    }
    const result = project.resolveModule(resolved.specifier, resolved.importer);
    if (this.#development)
      result.code += '\nif (import.meta.hot) import.meta.hot.accept();\n';
    result.watchFiles = union(
      result.watchFiles,
      this.#watchFiles.get(resolved.root),
    );
    this.#remember(id, result);
    this.#declarationHashes.set(
      id,
      declarationDigest(
        project.resolveDts(resolved.specifier, resolved.importer).code,
      ),
    );
    return result;
  }

  watchChange(
    path: string,
    text: string | null | undefined,
    host: BuildHost = {},
  ): void {
    // Only BAML sources and this project's own config ever enter the
    // compiler database. Feeding an arbitrary changed file (App.tsx under
    // vite dev) would poison the project with parse errors until restart;
    // the Rust session also rejects non-BAML paths defensively.
    const isBamlSource = path.endsWith('.baml');
    // The config match is exact, not a suffix: 'mybaml.toml' or a nested
    // 'sub/baml.toml' are not this project's baml.toml, and forwarding them
    // would make the Rust session reject the update with NonBamlPath.
    const isConfigName = basename(path) === 'baml.toml';
    if (!isBamlSource && !isConfigName) return;
    const root = this.#rootForPath(path);
    const project = root && this.#projects.get(root);
    if (!root || !project) return;
    const configChanged =
      isConfigName &&
      canonical(path) === canonical(project.layout().configPath);
    if (!isBamlSource && !configChanged) return;
    // A watch hook must never throw out of the bundler — an uncaught throw
    // kills rollup's watcher and vite's dev-server HMR. The failing path is
    // still reported through watchError, and the next load() re-runs the
    // compiler and surfaces the diagnostic through the normal error path.
    try {
      this.#applyWatchChange(root, project, path, text, host, configChanged);
    } catch (error) {
      (host.watchError ?? this.#host.watchError)?.(error);
    }
  }

  #applyWatchChange(
    root: string,
    project: BamlProject,
    path: string,
    text: string | null | undefined,
    host: BuildHost,
    configChanged: boolean,
  ): void {
    const sourceText =
      text === undefined
        ? (() => {
            try {
              return readFileSync(path, 'utf8');
            } catch {
              return null;
            }
          })()
        : text;
    project.updateFile(path, sourceText);
    project.refreshLayout();
    this.#registerWatch(root, project);
    const runtimeId = `\0baml:${this.#projectIds.get(root)}:runtime`;
    if (configChanged) {
      host.invalidate?.(runtimeId);
      host.fullReload?.();
      return;
    }
    const hasErrors = project
      .check()
      .diagnostics.some((diagnostic) => diagnostic.severity === 'error');
    if (hasErrors) {
      for (const [id, entry] of this.#resolved)
        if (entry.root === root) host.invalidate?.(id);
      return;
    }
    let shapeChanged = false;
    for (const [id, resolved] of this.#resolved) {
      if (resolved.root !== root) continue;
      const nextDeclaration = project.resolveDts(
        resolved.specifier,
        resolved.importer,
      ).code;
      const nextDeclarationHash = declarationDigest(nextDeclaration);
      if (
        this.#declarationHashes.has(id) &&
        this.#declarationHashes.get(id) !== nextDeclarationHash
      )
        shapeChanged = true;
      this.#declarationHashes.set(id, nextDeclarationHash);
      const next = project.resolveModule(resolved.specifier, resolved.importer);
      if (this.#hashes.get(id) !== digest(next.code)) host.invalidate?.(id);
    }
    host.invalidate?.(runtimeId);
    if (shapeChanged) this.#pendingShapeChange = true;
    if (this.#pendingShapeChange && host.fullReload) {
      host.fullReload();
      this.#pendingShapeChange = false;
    }
  }

  close(): void {
    for (const project of this.#projects.values()) project.dispose();
    this.#projects.clear();
    this.#opening.clear();
    this.#projectIds.clear();
    this.#idRoots.clear();
    this.#resolved.clear();
    this.#hashes.clear();
    this.#declarationHashes.clear();
    this.#lastGood.clear();
    this.#watchFiles.clear();
    this.#primaryRoot = '';
    this.#pendingShapeChange = false;
  }

  async #openProject(cwd: string, primary: boolean): Promise<string> {
    const project = await this.#factory({
      backend: 'auto',
      cacheDir:
        this.#options.cacheDir ?? (this.#development ? false : undefined),
      cwd,
      target: this.#options.target ?? 'node',
    });
    const root = canonical(project.layout().roots[0] ?? cwd);
    this.#projects.set(root, project);
    const id = projectId(root);
    this.#projectIds.set(root, id);
    this.#idRoots.set(id, root);
    if (primary) this.#primaryRoot = root;
    this.#registerWatch(root, project);
    return root;
  }

  #registerWatch(root: string, project: BamlProject): void {
    const watchFiles = project.layout().watchFiles;
    const previous = new Set(this.#watchFiles.get(root) ?? []);
    this.#watchFiles.set(root, watchFiles);
    for (const path of watchFiles)
      if (!previous.has(path)) this.#host.addWatchFile?.(path);
  }

  #resolveSpecifier(id: string, importer: string): string | undefined {
    if (isAbsolute(id)) return id;
    if (id.startsWith('.')) return resolve(dirname(importer), id);
    // Bare specifiers (`dep/baml_src/schema.baml`) resolve through Node's
    // module algorithm so dependencies inside node_modules — real installs
    // and pnpm-style symlinks alike — map to their physical location.
    try {
      return createRequire(importer).resolve(id);
    } catch {
      return undefined;
    }
  }

  async #owningRoot(specifier: string): Promise<string> {
    // The nearest baml.toml walking up from the import owns the source —
    // a dependency shipping its own baml_src + baml.toml inside the app's
    // node_modules, or a monorepo sibling reached through a symlinked
    // node_modules entry. A path-prefix match against open projects would
    // wrongly nest such dependencies inside the app's project.
    try {
      const root = discoverProject(dirname(canonical(specifier)));
      if (!this.#projects.has(root)) await this.#openProjectOnce(root);
      return root;
    } catch {
      return this.#rootForPath(specifier) ?? this.#primaryRoot;
    }
  }

  // Bundlers resolve concurrently; a dependency's project must open exactly
  // once even when two imports race, or the loser leaks a session and the
  // winner's virtual ids point at a disposed project.
  #openProjectOnce(root: string): Promise<string> {
    const pending = this.#opening.get(root);
    if (pending) return pending;
    const opening = this.#openProject(root, false).finally(() =>
      this.#opening.delete(root),
    );
    this.#opening.set(root, opening);
    return opening;
  }

  #rootForImporter(importer: string): string {
    return (
      this.#rootForPath(importer) ??
      (() => {
        try {
          const root = discoverProject(dirname(canonical(importer)));
          return this.#projects.has(root) ? root : this.#primaryRoot;
        } catch {
          return this.#primaryRoot;
        }
      })()
    );
  }

  #rootForPath(path: string): string | undefined {
    const target = canonical(path);
    let best: string | undefined;
    for (const root of this.#projects.keys())
      if (
        (target === root || target.startsWith(`${root}${sep}`)) &&
        (best === undefined || root.length > best.length)
      )
        best = root;
    return best;
  }

  #remember(id: string, result: { code: string; watchFiles: string[] }): void {
    this.#hashes.set(id, digest(result.code));
    this.#lastGood.set(id, result);
  }
}

function digest(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}

function declarationDigest(value: string): string {
  return digest(value.replace(/^\/\/ baml-fingerprint: [a-f0-9]+\r?\n/m, ''));
}

function union(left: string[], right: string[] | undefined): string[] {
  return [...new Set([...left, ...(right ?? [])])];
}

function canonical(path: string): string {
  try {
    return realpathSync(path);
  } catch {
    return resolve(path);
  }
}
