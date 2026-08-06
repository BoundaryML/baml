import { createHash } from 'node:crypto';
import { realpathSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve } from 'node:path';
import {
  BamlFileDiscovery,
  discoverProject,
  LineIndexCache,
  resolveBamlSpecifier,
  utf8OffsetToUtf16,
} from '@boundaryml/baml-tooling';
import type ts from 'typescript/lib/tsserverlibrary';

interface PluginConfig {
  target?: 'node' | 'web';
  debug?: boolean;
  root?: string;
}

type BamlProject = import('@boundaryml/baml-tooling').BamlProject;

type ScriptInfoView = {
  fileName?: string;
  getLatestVersion?: () => string;
  getSnapshot?: () => ts.IScriptSnapshot;
  isScriptOpen?: () => boolean;
};

interface VirtualFile {
  fileName: string;
  specifier: string;
  importer: string;
  /** Physical `.baml` path the specifier resolved to, when it resolved at
   * all. The owning lane tracks it as a discovery candidate so an import of
   * a host-only source (an unsaved buffer) still reaches the compiler. */
  source?: string;
  /** Canonical root of the compiler session that owns the resolved source. */
  laneRoot: string;
  declaration: string;
  version: number;
  map?: import('@boundaryml/baml-tooling').SegmentMap;
  scriptInfo?: ts.server.ScriptInfo;
  stale: boolean;
  /** Set when resolution or emission failed; surfaced as an import-span
   * diagnostic so a broken import never renders as a silent empty module. */
  error?: string;
}

/** One compiler session (app project or dependency project) plus the
 * overlay/discovery state the plugin maintains for it. */
interface ProjectLane {
  readonly root: string;
  readonly project: BamlProject;
  readonly discovery: BamlFileDiscovery;
  readonly sourceVersions: Map<string, string>;
  configVersion: string;
}

const INITIALIZING = 'declare const b: never; export { b };\n';
const DIAGNOSTIC_BASE = 91000;

/** Whether `path` belongs in a recursive `.baml` listing rooted at
 * `directory`, applying the same exclusions the disk walk applies so a
 * dependency's own sources stay in the dependency's lane. */
function withinListing(directory: string, path: string): boolean {
  const inside = relative(directory, path);
  if (inside === '' || inside.startsWith('..') || isAbsolute(inside))
    return false;
  return !inside
    .split(/[\\/]/)
    .some(
      (segment) =>
        segment === 'node_modules' || segment === '.git' || segment === '.baml',
    );
}

function init({
  typescript,
}: {
  typescript: typeof ts;
}): ts.server.PluginModule {
  const projects = new WeakMap<ts.server.Project, PluginState>();

  class PluginState {
    readonly virtualFiles = new Map<string, VirtualFile>();
    readonly proxy: ts.LanguageService;
    /** Compiler sessions by canonical project root: the app's own project
     * plus one lane per dependency/sibling project reached through a bare
     * `.baml` import (node_modules package or pnpm workspace sibling). */
    readonly lanes = new Map<string, ProjectLane>();
    readonly laneErrors = new Map<string, Error>();
    readonly lineIndexes = new LineIndexCache();
    readonly primaryRoot: string;
    #opening = new Map<string, Promise<void>>();
    #laneTickets = new Map<string, number>();

    get project(): BamlProject | undefined {
      return this.lanes.get(this.primaryRoot)?.project;
    }

    get error(): Error | undefined {
      return this.laneErrors.get(this.primaryRoot);
    }

    constructor(
      readonly info: ts.server.PluginCreateInfo,
      readonly config: PluginConfig,
    ) {
      const start = this.config.root ?? this.info.project.getCurrentDirectory();
      this.primaryRoot = this.discoverRoot(start) ?? start;
      this.decorateHost();
      this.proxy = this.decorateLanguageService();
      this.openLane(this.primaryRoot);
    }

    /** Nearest `baml.toml` root walking up from `start`, or undefined when
     * none is visible to the host (loadProject then fails loudly). */
    discoverRoot(start: string): string | undefined {
      try {
        return discoverProject(
          start,
          (path) => this.info.languageServiceHost.fileExists?.(path) ?? false,
        );
      } catch {
        return undefined;
      }
    }

    /** Opens (or, with reopen, replaces) the compiler session for a project
     * root. Single-flighted per root so racing imports open one session. */
    openLane(root: string, reopen = false): void {
      if (!reopen && (this.lanes.has(root) || this.#opening.has(root))) return;
      const ticket = (this.#laneTickets.get(root) ?? 0) + 1;
      this.#laneTickets.set(root, ticket);
      const task = (async () => {
        let loaded: BamlProject | undefined;
        try {
          const { loadProject } = await import('@boundaryml/baml-tooling');
          // One file-access port for the whole lane: the compiler session and
          // the discovery poller must see the same files, and both must see
          // the *host's* view (unsaved buffers included), never Node's.
          const access = this.hostAccess();
          loaded = await loadProject({
            ...access,
            backend: 'auto',
            cwd: root,
            target: this.config.target ?? 'node',
          });
          if (this.#laneTickets.get(root) !== ticket) {
            // A newer open superseded this one while loading; release the
            // freshly loaded native session instead of leaking it.
            loaded.dispose();
            return;
          }
          // Replacing the lane (config change reopen) must release the
          // previous native session instead of leaking it.
          const previous = this.lanes.get(root);
          const lane: ProjectLane = {
            configVersion:
              this.info.languageServiceHost.getScriptVersion?.(
                loaded.layout().configPath,
              ) ?? '0',
            discovery: new BamlFileDiscovery(
              loaded.layout().roots[0] ?? root,
              access,
            ),
            project: loaded,
            root,
            sourceVersions: new Map(),
          };
          for (const source of loaded.layout().sourceFiles)
            lane.sourceVersions.set(source, this.hostScriptVersion(source));
          this.lanes.set(root, lane);
          this.laneErrors.delete(root);
          previous?.project.dispose();
          for (const virtual of this.virtualFiles.values())
            if (virtual.laneRoot === root) this.refreshVirtual(virtual);
          this.invalidate();
        } catch (error) {
          const failure =
            error instanceof Error ? error : new Error(String(error));
          this.laneErrors.set(root, failure);
          this.log(failure.message);
          for (const virtual of this.virtualFiles.values())
            if (virtual.laneRoot === root) {
              virtual.error = failure.message;
              virtual.stale = true;
            }
          this.invalidate();
        } finally {
          this.#opening.delete(root);
        }
      })();
      this.#opening.set(root, task);
    }

    /** The language-service host projected onto the tooling package's file
     * abstraction. Everything the compiler and discovery read goes through
     * here, so an editor's in-memory view — an unsaved `.baml` buffer that
     * `statSync` cannot see — is the view the compiler gets. */
    hostAccess(): import('@boundaryml/baml-tooling').FileAccess {
      const host = this.info.languageServiceHost as ts.LanguageServiceHost & {
        readDirectory?: (
          path: string,
          extensions?: readonly string[],
          exclude?: readonly string[],
          include?: readonly string[],
          depth?: number,
        ) => string[];
      };
      return {
        // An unsaved buffer exists for the user, so it must exist for the
        // compiler: `Project.fileExists` falls through to the process
        // filesystem for paths it does not already track. Asking the
        // ProjectService by name reuses tsserver's own path normalization
        // instead of string-matching against the listing.
        fileExists: (path) => this.hostFileExists(path),
        readDirectory: (directory) => {
          const listed = new Set<string>();
          try {
            for (const path of host.readDirectory?.(
              directory,
              ['.baml'],
              ['node_modules', '.git', '.baml'],
              ['**/*'],
            ) ?? [])
              listed.add(path);
          } catch {
            // A host with no disk-backed listing still contributes the
            // editor's own view below.
          }
          for (const path of this.openBamlBuffers())
            if (withinListing(directory, path)) listed.add(path);
          return [...listed];
        },
        readFile: (path) => this.snapshotText(path) ?? host.readFile?.(path),
      };
    }

    /**
     * The `.baml` paths the editor currently has open, including buffers that
     * were never written to disk. tsserver's `readDirectory` is backed by the
     * process filesystem, so a brand-new unsaved buffer is absent from it even
     * while the editor serves its text — `ProjectService` is the only host
     * channel that knows about it. Without this the compiler's file set would
     * silently be the disk's, not the user's, and a new buffer would stay
     * invisible until it was saved or imported.
     *
     * Recomputed per call (the set is the count of open editor tabs, and a
     * cache would have to be invalidated on every open/close) and defensive:
     * tsserver internals that are absent or renamed degrade to the disk-only
     * listing that was there before, never to an exception.
     */
    openBamlBuffers(): Set<string> {
      const buffers = new Set<string>();
      try {
        for (const key of this.projectService().openFiles?.keys() ?? []) {
          // `openFiles` is keyed by tsserver's canonical `Path` — lowercased
          // on case-insensitive filesystems — so the real casing has to come
          // back off the ScriptInfo rather than out of the key.
          const name = this.scriptInfoFor(key)?.fileName ?? key;
          if (name.endsWith('.baml')) buffers.add(name);
        }
      } catch {
        // Absent or renamed tsserver internals degrade to the disk-only
        // listing that was here before, never to a thrown request.
      }
      return buffers;
    }

    /** Whether the editor is holding a buffer for `path`, saved or not. */
    hasOpenBuffer(path: string): boolean {
      try {
        // tsserver retains ScriptInfo objects for closed project/external
        // files. Only ScriptInfo's own open bit means an editor buffer is
        // still authoritative over a missing disk file.
        return this.scriptInfoFor(path)?.isScriptOpen?.() === true;
      } catch {
        return false;
      }
    }

    /** Existence as the host sees it: on disk, or open in the editor. */
    hostFileExists(path: string): boolean {
      return (
        (this.info.languageServiceHost.fileExists?.(path) ?? false) ||
        this.hasOpenBuffer(path)
      );
    }

    /** The version that moves when the user edits `path`. A `.baml` buffer is
     * not a script of the TypeScript project, so the language-service host has
     * no version for it and would report a frozen `'0'` forever — edits to an
     * unsaved buffer would then never reach the compiler. The editor's own
     * ScriptInfo carries the version that actually moves. */
    hostScriptVersion(path: string): string {
      try {
        const buffered = (
          this.scriptInfoFor(path) as
            | { getLatestVersion?: () => string }
            | undefined
        )?.getLatestVersion?.();
        if (buffered !== undefined) return buffered;
      } catch {
        // Fall through to the language-service host's own version.
      }
      return this.info.languageServiceHost.getScriptVersion?.(path) ?? '0';
    }

    projectService(): {
      openFiles?: ReadonlyMap<string, unknown>;
      getScriptInfoForPath?: (path: string) => ScriptInfoView | undefined;
      getScriptInfo?: (name: string) => ScriptInfoView | undefined;
    } {
      return this.info.project.projectService as unknown as ReturnType<
        PluginState['projectService']
      >;
    }

    scriptInfoFor(pathOrName: string): ScriptInfoView | undefined {
      const service = this.projectService();
      return (
        service.getScriptInfoForPath?.(pathOrName) ??
        service.getScriptInfo?.(pathOrName)
      );
    }

    decorateHost(): void {
      const host = this.info.languageServiceHost;
      const resolveModuleNameLiterals =
        host.resolveModuleNameLiterals?.bind(host);
      host.resolveModuleNameLiterals = (
        literals,
        containingFile,
        redirectedReference,
        options,
        containingSourceFile,
        reusedNames,
      ) => {
        const original =
          resolveModuleNameLiterals?.(
            literals,
            containingFile,
            redirectedReference,
            options,
            containingSourceFile,
            reusedNames,
          ) ?? literals.map(() => ({ resolvedModule: undefined }));
        return literals.map((literal, index) => {
          const result = original[index];
          if (
            result?.resolvedModule ||
            (literal.text !== 'baml:client' && !literal.text.endsWith('.baml'))
          )
            return result;
          const virtual = this.virtual(literal.text, containingFile);
          return {
            resolvedModule: {
              extension: typescript.Extension.Dts,
              isExternalLibraryImport: false,
              resolvedFileName: virtual.fileName,
            },
          };
        });
      };

      const getScriptSnapshot = host.getScriptSnapshot.bind(host);
      host.getScriptSnapshot = (fileName) => {
        const virtual = this.virtualFiles.get(fileName);
        return virtual
          ? typescript.ScriptSnapshot.fromString(virtual.declaration)
          : getScriptSnapshot(fileName);
      };
      const getScriptVersion = host.getScriptVersion?.bind(host);
      host.getScriptVersion = (fileName) =>
        this.virtualFiles.get(fileName)?.version.toString() ??
        getScriptVersion?.(fileName) ??
        '0';
      const fileExists = host.fileExists?.bind(host);
      host.fileExists = (fileName) =>
        this.virtualFiles.has(fileName) || fileExists?.(fileName) || false;
      const readFile = host.readFile?.bind(host);
      host.readFile = (fileName) =>
        this.virtualFiles.get(fileName)?.declaration ?? readFile?.(fileName);
      const getScriptKind = host.getScriptKind?.bind(host);
      host.getScriptKind = (fileName) =>
        this.virtualFiles.has(fileName)
          ? typescript.ScriptKind.TS
          : (getScriptKind?.(fileName) ?? typescript.ScriptKind.Unknown);
    }

    decorateLanguageService(): ts.LanguageService {
      const languageService = this.info.languageService;
      const proxy = Object.create(null) as ts.LanguageService;
      for (const key of Object.keys(
        languageService,
      ) as (keyof ts.LanguageService)[]) {
        const value = languageService[key];
        (proxy as unknown as Record<string, unknown>)[key] =
          typeof value === 'function' ? value.bind(languageService) : value;
      }

      proxy.getDefinitionAtPosition = (fileName, position) => {
        this.syncOverlays();
        const serviceFile = this.serviceFileName(fileName);
        return this.definitions(
          fileName,
          position,
          languageService.getDefinitionAtPosition(serviceFile, position) ?? [],
        );
      };
      proxy.getDefinitionAndBoundSpan = (fileName, position) => {
        this.syncOverlays();
        const serviceFile = this.serviceFileName(fileName);
        const result = languageService.getDefinitionAndBoundSpan(
          serviceFile,
          position,
        );
        const definitions = this.definitions(
          fileName,
          position,
          result?.definitions ?? [],
        );
        return definitions.length
          ? {
              definitions,
              textSpan: result?.textSpan ?? { length: 0, start: position },
            }
          : undefined;
      };
      proxy.getReferencesAtPosition = (fileName, position) => {
        this.syncOverlays();
        const serviceFile = this.serviceFileName(fileName);
        return this.referenceEntries(
          fileName,
          position,
          languageService.getReferencesAtPosition(serviceFile, position) ?? [],
        );
      };
      proxy.findReferences = (fileName, position) => {
        this.syncOverlays();
        const serviceFile = this.serviceFileName(fileName);
        const entries = this.referenceEntries(
          fileName,
          position,
          languageService.getReferencesAtPosition(serviceFile, position) ?? [],
        );
        const definition = this.definitions(
          fileName,
          position,
          languageService.getDefinitionAtPosition(serviceFile, position) ?? [],
        )[0];
        return definition && entries.length
          ? [
              {
                definition: {
                  ...definition,
                  displayParts: [{ kind: 'text', text: definition.name }],
                },
                references: entries,
              },
            ]
          : undefined;
      };
      proxy.getRenameInfo = (fileName, position, options) => {
        this.syncOverlays();
        const serviceFile = this.serviceFileName(fileName);
        const original = languageService.getRenameInfo(
          serviceFile,
          position,
          options,
        );
        const mapped = this.symbolAt(fileName, position);
        if (!mapped) return original;
        if (!mapped.project.capabilities().features.includes('rename.v1'))
          return {
            canRename: false,
            localizedErrorMessage:
              'rename across the BAML boundary requires a newer BAML compiler bridge',
          };
        try {
          const location = mapped.project.prepareRename(mapped.symbolId);
          return {
            canRename: true,
            displayName: original.canRename
              ? original.displayName
              : mapped.symbolId,
            fileToRename: undefined,
            fullDisplayName: original.canRename
              ? original.fullDisplayName
              : mapped.symbolId,
            kind: original.canRename
              ? original.kind
              : typescript.ScriptElementKind.unknown,
            kindModifiers: original.canRename ? original.kindModifiers : '',
            triggerSpan: original.canRename
              ? original.triggerSpan
              : this.toTextSpan(location),
          };
        } catch (error) {
          return { canRename: false, localizedErrorMessage: String(error) };
        }
      };
      proxy.findRenameLocations = (
        fileName,
        position,
        findInStrings,
        findInComments,
        preferences,
      ) => {
        this.syncOverlays();
        const serviceFile = this.serviceFileName(fileName);
        const original =
          (
            languageService.findRenameLocations as (
              ...args: unknown[]
            ) => readonly ts.RenameLocation[] | undefined
          )(
            serviceFile,
            position,
            findInStrings,
            findInComments,
            preferences,
          ) ?? [];
        const mapped = this.symbolAt(fileName, position);
        if (!mapped) return original;
        try {
          mapped.project.prepareRename(mapped.symbolId);
          const compiler = mapped.project
            .referencesAt('', 0, mapped.symbolId)
            .concat(mapped.project.definitionAt('', 0, mapped.symbolId))
            .map((location) => ({
              fileName: location.path,
              textSpan: this.toTextSpan(location),
            }));
          // A failed map round-trip refuses the whole operation: emitting a
          // partial rename would silently corrupt the project.
          const translated = this.translateRename(original, true);
          if (!translated) return undefined;
          return dedupe(
            translated.concat(compiler),
            (entry) =>
              `${entry.fileName}:${entry.textSpan.start}:${entry.textSpan.length}`,
          );
        } catch {
          return undefined;
        }
      };
      proxy.getSemanticDiagnostics = (fileName) => {
        this.syncOverlays();
        const serviceFile = this.serviceFileName(fileName);
        const diagnostics = languageService
          .getSemanticDiagnostics(serviceFile)
          .filter(
            (diagnostic) =>
              !this.virtualFiles.has(diagnostic.file?.fileName ?? ''),
          );
        if (this.lanes.size === 0) {
          if (this.error)
            diagnostics.push({
              category: typescript.DiagnosticCategory.Error,
              code: DIAGNOSTIC_BASE,
              file: languageService.getProgram()?.getSourceFile(serviceFile),
              length: 0,
              messageText: this.error.message,
              start: 0,
            });
          else
            diagnostics.push({
              category: typescript.DiagnosticCategory.Suggestion,
              code: DIAGNOSTIC_BASE + 1,
              file: languageService.getProgram()?.getSourceFile(serviceFile),
              length: 0,
              messageText: 'BAML tooling initializing',
              start: 0,
            });
          return diagnostics;
        }
        const program = languageService.getProgram();
        const sourceFile = program?.getSourceFile(serviceFile);
        for (const lane of this.lanes.values()) {
          const check = lane.project.check();
          for (const diagnostic of check.diagnostics) {
            if (diagnostic.location?.path === fileName) {
              const span = this.toTextSpan(diagnostic.location);
              diagnostics.push({
                category:
                  diagnostic.severity === 'error'
                    ? typescript.DiagnosticCategory.Error
                    : typescript.DiagnosticCategory.Warning,
                code: DIAGNOSTIC_BASE + 2,
                file: sourceFile,
                length: span.length,
                messageText: diagnostic.message,
                start: span.start,
              });
            } else if (sourceFile) {
              for (const span of bamlImportSpans(
                typescript,
                sourceFile,
                diagnostic.location?.path,
              ))
                diagnostics.push({
                  category: typescript.DiagnosticCategory.Error,
                  code: DIAGNOSTIC_BASE + 2,
                  file: sourceFile,
                  length: span.length,
                  messageText: `${diagnostic.location?.path ?? 'BAML'}: ${diagnostic.message}`,
                  start: span.start,
                });
            }
          }
          for (const source of lane.project.layout().sourceFiles) {
            // Both override forms embed the fingerprint: `<name>.baml.ts`
            // (runtime and types) and `<name>.baml.d.ts` (declarations only).
            const sidecar =
              this.info.languageServiceHost.readFile?.(`${source}.d.ts`) ??
              this.info.languageServiceHost.readFile?.(`${source}.ts`);
            const fingerprint =
              sidecar &&
              /^\/\/ baml-fingerprint: ([a-f0-9]+)$/m.exec(sidecar)?.[1];
            if (
              sidecar &&
              fingerprint !== lane.project.fingerprint() &&
              sourceFile
            )
              for (const span of bamlImportSpans(
                typescript,
                sourceFile,
                source,
              ))
                diagnostics.push({
                  category: typescript.DiagnosticCategory.Warning,
                  code: DIAGNOSTIC_BASE + 3,
                  file: sourceFile,
                  length: span.length,
                  messageText: `stale BAML sidecar for ${source} — run baml-ts-gen`,
                  start: span.start,
                });
          }
        }
        // A BAML import that failed to resolve or emit must be loud: an
        // empty declaration would otherwise render as silently missing types
        // while the bundler build still fails (or worse, succeeds).
        if (sourceFile)
          for (const virtual of this.virtualFiles.values()) {
            if (
              !virtual.error ||
              (virtual.importer !== serviceFile &&
                virtual.importer !== fileName)
            )
              continue;
            for (const span of bamlImportSpans(
              typescript,
              sourceFile,
              undefined,
              virtual.specifier,
            ))
              diagnostics.push({
                category: typescript.DiagnosticCategory.Error,
                code: DIAGNOSTIC_BASE + 4,
                file: sourceFile,
                length: span.length,
                messageText: virtual.error,
                start: span.start,
              });
          }
        return diagnostics;
      };
      proxy.getCompletionsAtPosition = (
        fileName,
        position,
        options,
        formattingSettings,
      ) => {
        this.syncOverlays();
        const serviceFile = this.serviceFileName(fileName);
        const result = languageService.getCompletionsAtPosition(
          serviceFile,
          position,
          options,
          formattingSettings,
        ) ?? {
          entries: [],
          isGlobalCompletion: false,
          isMemberCompletion: false,
          isNewIdentifierLocation: false,
        };
        const sourceFile = languageService
          .getProgram()
          ?.getSourceFile(serviceFile);
        if (
          this.lanes.size > 0 &&
          sourceFile &&
          isModuleSpecifierPosition(typescript, sourceFile, position)
        ) {
          const specifiers = [
            'baml:client',
            ...[...this.lanes.values()].flatMap((lane) =>
              lane.project
                .layout()
                .sourceFiles.map((source) =>
                  relativeBamlSpecifier(fileName, source),
                ),
            ),
          ];
          for (const name of specifiers)
            if (!result.entries.some((entry) => entry.name === name))
              result.entries.push({
                kind: typescript.ScriptElementKind.externalModuleName,
                name,
                sortText: '0',
                source: 'baml',
              });
        }
        return result;
      };
      proxy.getCompletionEntryDetails = (
        fileName,
        position,
        name,
        formatOptions,
        source,
        preferences,
        data,
      ) => {
        this.syncOverlays();
        const serviceFile = this.serviceFileName(fileName);
        const detail = languageService.getCompletionEntryDetails(
          serviceFile,
          position,
          name,
          formatOptions,
          source,
          preferences,
          data,
        );
        const mapped = this.symbolAt(fileName, position);
        if (!detail || !mapped) return detail;
        try {
          const hover = mapped.project.hoverAt('', 0, mapped.symbolId);
          return {
            ...detail,
            documentation: [
              ...(detail.documentation ?? []),
              { kind: 'text', text: hover.markdown },
            ],
          };
        } catch {
          return detail;
        }
      };
      proxy.getQuickInfoAtPosition = (fileName, position) => {
        this.syncOverlays();
        const serviceFile = this.serviceFileName(fileName);
        const info = languageService.getQuickInfoAtPosition(
          serviceFile,
          position,
        );
        const mapped = this.symbolAt(fileName, position);
        if (!info || !mapped) return info;
        try {
          const hover = mapped.project.hoverAt('', 0, mapped.symbolId);
          return {
            ...info,
            documentation: [
              ...(info.documentation ?? []),
              {
                kind: 'text',
                text: `\n\n${hover.markdown}\n\nDeclared in ${hover.location?.path ?? 'BAML'}`,
              },
            ],
          };
        } catch {
          return info;
        }
      };
      return proxy;
    }

    virtual(specifier: string, importer: string): VirtualFile {
      const root = this.config.root ?? this.info.project.getCurrentDirectory();
      // Identity is the resolved physical source, never the raw specifier:
      // two files in different directories importing `./schema.baml` name
      // different BAML sources and must get distinct virtual files. Bare
      // specifiers (`dep/baml_src/widget.baml`) go through Node's module
      // algorithm so a node_modules dependency or pnpm symlink lands on its
      // physical path — joining them onto the importer's directory would
      // invent a path that does not exist.
      const resolved = resolveBamlSpecifier(specifier, importer);
      const identity =
        resolved ??
        `unresolved\0${specifier}\0${canonicalPath(dirname(importer))}`;
      // A full cryptographic digest keeps distinct physical sources from
      // aliasing the same virtual declaration. The previous 32-bit FNV key
      // had known collisions for same-basename sources.
      const hash = createHash('sha256')
        .update(`${root}\0${identity}`)
        .digest('hex');
      const base =
        resolved === 'baml:client'
          ? 'client'
          : (identity
              .split(/[\\/]/)
              .pop()
              ?.replace(/[^a-zA-Z0-9_.-]/g, '_') ?? 'module.baml');
      const fileName = `${root}/.baml/__virtual__/p_${hash}/${base}.d.ts`;
      let virtual = this.virtualFiles.get(fileName);
      if (!virtual) {
        // The nearest baml.toml walking up from the resolved source owns it;
        // a dependency shipping its own baml.toml gets its own session.
        const laneRoot =
          resolved === undefined || resolved === 'baml:client'
            ? this.primaryRoot
            : (this.discoverRoot(dirname(resolved)) ?? this.primaryRoot);
        virtual = {
          declaration: INITIALIZING,
          fileName,
          importer,
          laneRoot,
          source:
            resolved === undefined || resolved === 'baml:client'
              ? undefined
              : resolved,
          specifier,
          stale: false,
          version: 0,
        };
        if (resolved === undefined)
          virtual.error = `Could not resolve BAML import '${specifier}' from ${importer} through Node module resolution`;
        this.virtualFiles.set(fileName, virtual);
        this.ensureVirtualScriptInfo(virtual);
        if (!this.lanes.has(laneRoot)) this.openLane(laneRoot);
        if (!virtual.error) this.refreshVirtual(virtual);
      }
      return virtual;
    }

    refreshVirtual(virtual: VirtualFile): void {
      const lane = this.lanes.get(virtual.laneRoot);
      if (!lane) return;
      const previous = virtual.declaration;
      try {
        const artifact = lane.project.resolveDts(
          virtual.specifier,
          virtual.importer,
        );
        virtual.declaration = artifact.code;
        virtual.map = artifact.map;
        virtual.stale = artifact.stale;
        virtual.error = undefined;
      } catch (error) {
        virtual.declaration = INITIALIZING;
        virtual.map = undefined;
        virtual.stale = true;
        virtual.error = error instanceof Error ? error.message : String(error);
      }
      virtual.version++;
      if (virtual.declaration !== previous && virtual.scriptInfo) {
        virtual.scriptInfo.editContent(
          0,
          virtual.scriptInfo.getSnapshot().getLength(),
          virtual.declaration,
        );
      }
      this.ensureVirtualScriptInfo(virtual);
    }

    ensureVirtualScriptInfo(virtual: VirtualFile): void {
      const projectService = this.info.project
        .projectService as ts.server.ProjectService & {
        getOrCreateScriptInfoForNormalizedPath?: ts.server.ProjectService['getOrCreateScriptInfoForNormalizedPath'];
        getScriptInfo?: ts.server.ProjectService['getScriptInfo'];
      };
      if (!projectService.getOrCreateScriptInfoForNormalizedPath) return;
      virtual.scriptInfo ??=
        projectService.getScriptInfo?.(virtual.fileName) ??
        projectService.getOrCreateScriptInfoForNormalizedPath(
          typescript.server.toNormalizedPath(virtual.fileName),
          true,
          virtual.declaration,
          typescript.ScriptKind.TS,
          false,
          { fileExists: (path) => path === virtual.fileName },
        );
      virtual.scriptInfo?.attachToProject(this.info.project);
    }

    definitions(
      fileName: string,
      position: number,
      original: readonly ts.DefinitionInfo[],
    ): ts.DefinitionInfo[] {
      const output: ts.DefinitionInfo[] = [];
      let symbolId: string | undefined;
      let owner: BamlProject | undefined;
      for (const definition of original) {
        const virtual = this.virtualFiles.get(definition.fileName);
        if (!virtual) {
          output.push(definition);
          continue;
        }
        const mapped =
          virtual.map && mapGenerated(virtual.map, definition.textSpan.start);
        if (mapped) {
          symbolId = mapped.symbolId;
          owner = this.lanes.get(virtual.laneRoot)?.project ?? owner;
        }
      }
      const fallback = symbolId ? undefined : this.symbolAt(fileName, position);
      symbolId ??= fallback?.symbolId;
      owner ??= fallback?.project;
      if (symbolId && owner)
        for (const location of owner.definitionAt('', 0, symbolId))
          output.push({
            containerKind: typescript.ScriptElementKind.unknown,
            containerName: '',
            fileName: location.path,
            kind: typescript.ScriptElementKind.unknown,
            name: symbolId,
            textSpan: this.toTextSpan(location),
          });
      return dedupe(
        output,
        (entry) =>
          `${entry.fileName}:${entry.textSpan.start}:${entry.textSpan.length}`,
      );
    }

    referenceEntries(
      fileName: string,
      position: number,
      original: readonly ts.ReferenceEntry[],
    ): ts.ReferenceEntry[] {
      const mapped = this.symbolAt(fileName, position);
      // TS-side entries keep their own write-access flags; only
      // compiler-sourced BAML locations default to read references.
      const output: ts.ReferenceEntry[] = this.translateRename(original) ?? [];
      if (mapped)
        for (const location of mapped.project
          .referencesAt('', 0, mapped.symbolId)
          .concat(mapped.project.definitionAt('', 0, mapped.symbolId)))
          output.push({
            fileName: location.path,
            isWriteAccess: false,
            textSpan: this.toTextSpan(location),
          });
      return dedupe(
        output,
        (entry) =>
          `${entry.fileName}:${entry.textSpan.start}:${entry.textSpan.length}`,
      );
    }

    translateRename<T extends { fileName: string; textSpan: ts.TextSpan }>(
      entries: readonly T[],
      strict = false,
    ): T[] | undefined {
      const output: T[] = [];
      for (const entry of entries) {
        const virtual = this.virtualFiles.get(entry.fileName);
        if (!virtual) output.push(entry);
        else if (virtual.map) {
          const mapped = mapGenerated(virtual.map, entry.textSpan.start);
          if (mapped)
            output.push({
              ...entry,
              fileName: mapped.path,
              textSpan: this.toTextSpan(mapped),
            });
          else if (strict) return undefined;
        } else if (strict) return undefined;
      }
      return output;
    }

    symbolAt(
      fileName: string,
      position: number,
    ): { symbolId: string; project: BamlProject } | undefined {
      const direct = this.virtualFiles.get(fileName);
      if (direct?.map) {
        const mapped = mapGenerated(direct.map, position);
        const lane = this.lanes.get(direct.laneRoot);
        if (mapped && lane) return { ...mapped, project: lane.project };
      }
      const definitions =
        this.info.languageService.getDefinitionAtPosition(
          this.serviceFileName(fileName),
          position,
        ) ?? [];
      for (const definition of definitions) {
        const virtual = this.virtualFiles.get(definition.fileName);
        if (!virtual?.map) continue;
        const mapped = mapGenerated(virtual.map, definition.textSpan.start);
        const lane = this.lanes.get(virtual.laneRoot);
        if (mapped && lane) return { ...mapped, project: lane.project };
      }
      return undefined;
    }

    toTextSpan(location: {
      path: string;
      startUtf8: number;
      lengthUtf8: number;
    }): ts.TextSpan {
      // The located file's text may be unavailable (e.g. a just-deleted
      // source); degrade to an empty span instead of throwing a RangeError
      // that would fail the entire tsserver request.
      try {
        const text =
          this.snapshotText(location.path) ??
          this.laneSourceText(location.path) ??
          '';
        // One index per (file, snapshot): editor requests translate many
        // locations against the same unchanging text.
        const index = this.lineIndexes.forText(location.path, text);
        const start = utf8OffsetToUtf16(index, location.startUtf8);
        const end = utf8OffsetToUtf16(
          index,
          location.startUtf8 + location.lengthUtf8,
        );
        return { length: end - start, start };
      } catch {
        return { length: 0, start: 0 };
      }
    }

    snapshotText(path: string): string | undefined {
      // A `.baml` buffer is open in the editor but is not a script of *this*
      // TypeScript project, so the language-service host may not carry its
      // snapshot; the ProjectService always does. Listing a buffer the
      // compiler then could not read would be worse than not listing it.
      const snapshot =
        this.info.languageServiceHost.getScriptSnapshot(path) ??
        this.bufferSnapshot(path);
      return snapshot?.getText(0, snapshot.getLength());
    }

    bufferSnapshot(path: string): ts.IScriptSnapshot | undefined {
      try {
        return (
          this.scriptInfoFor(path) as
            | { getSnapshot?: () => ts.IScriptSnapshot }
            | undefined
        )?.getSnapshot?.();
      } catch {
        return undefined;
      }
    }

    laneSourceText(path: string): string | undefined {
      for (const lane of this.lanes.values()) {
        const text = lane.project.sourceText(path);
        if (text !== undefined) return text;
      }
      return undefined;
    }
    serviceFileName(fileName: string): string {
      const canonical = fileName.toLowerCase();
      const sourceFile = this.info.languageService
        .getProgram?.()
        ?.getSourceFiles()
        .find(
          (candidate) =>
            candidate.fileName === fileName ||
            candidate.fileName.toLowerCase() === canonical,
        );
      if (sourceFile) return sourceFile.fileName;
      const projectFile = (
        this.info.project as unknown as {
          getFileNames?: () => readonly string[];
        }
      )
        .getFileNames?.()
        .find(
          (candidate) =>
            candidate === fileName || candidate.toLowerCase() === canonical,
        );
      if (projectFile) return projectFile;
      return typescript.sys.useCaseSensitiveFileNames ? fileName : canonical;
    }
    syncOverlays(): void {
      for (const lane of this.lanes.values()) this.syncLane(lane);
    }

    syncLane(lane: ProjectLane): void {
      const configPath = lane.project.layout().configPath;
      const configVersion =
        this.info.languageServiceHost.getScriptVersion?.(configPath) ?? '0';
      if (configVersion !== lane.configVersion) {
        lane.configVersion = configVersion;
        this.openLane(lane.root, true);
        return;
      }
      let changed = false;
      // Cheap per-request path: version counters catch edits and existence
      // catches deletions for files the session already knows. This is
      // O(sources) map lookups and stats — no directory walk, no re-reads.
      for (const source of [...lane.sourceVersions.keys()]) {
        // Existence is the host's, not the disk's: a source that lives only
        // in an editor buffer is not a deleted file, and sweeping it as one
        // would drop it from the compiler on the very next request — right
        // after discovery had just added it.
        if (!this.hostFileExists(source)) {
          // Deleted sources leave the compiler database so phantom symbols
          // cannot linger.
          lane.project.updateFile(source, null);
          lane.sourceVersions.delete(source);
          changed = true;
          continue;
        }
        const version = this.hostScriptVersion(source);
        if (lane.sourceVersions.get(source) === version) continue;
        const text =
          this.snapshotText(source) ??
          this.info.languageServiceHost.readFile?.(source);
        if (text === undefined) continue;
        lane.project.updateFile(source, text);
        lane.sourceVersions.set(source, version);
        changed = true;
      }
      // A source the editor created but never saved exists only in the host:
      // no directory stamp moves and no filesystem walk can see it. The
      // import that named it is the evidence it exists, so every resolved
      // import target is offered to the poller as a discovery candidate.
      for (const virtual of this.virtualFiles.values())
        if (virtual.laneRoot === lane.root && virtual.source)
          lane.discovery.track(virtual.source);
      // The full recursive walk (adds and removals) is gated: it runs only
      // when a directory mtime moved, a tracked candidate appeared, or the
      // host's own listing changed — never on every keystroke. That listing
      // is compared on every poll, not merely read inside a re-walk, so a
      // buffer the host created but nothing imports yet still reaches the
      // compiler instead of waiting for a save or an unrelated disk change.
      const discovered = lane.discovery.poll();
      if (discovered) {
        this.log(
          `rediscovered ${discovered.size} .baml files under ${lane.root}`,
        );
        for (const source of discovered) {
          if (lane.sourceVersions.has(source)) continue;
          const text =
            this.snapshotText(source) ??
            this.info.languageServiceHost.readFile?.(source);
          if (text === undefined) continue;
          lane.project.updateFile(source, text);
          lane.sourceVersions.set(source, this.hostScriptVersion(source));
          changed = true;
        }
      }
      if (changed) {
        lane.project.refreshLayout();
        for (const virtual of this.virtualFiles.values())
          if (virtual.laneRoot === lane.root) this.refreshVirtual(virtual);
        // Rewriting a virtual declaration is not enough on its own: the
        // language service keeps serving the program it already built until
        // the project version moves. Without this the `baml:client` type, the
        // semantic diagnostics, and the member completions would keep
        // describing the file set as it was before discovery ran — stale in
        // exactly the way discovery just fixed.
        this.invalidate();
      }
    }
    invalidate(): void {
      const project = this.info.project as unknown as {
        markAsDirty?: () => void;
        refreshDiagnostics?: () => void;
      };
      project.markAsDirty?.();
      project.refreshDiagnostics?.();
    }
    log(message: string): void {
      if (this.config.debug)
        this.info.project.projectService.logger.info(
          `[baml-tooling] ${message}`,
        );
    }
  }

  return {
    create(info) {
      const state = new PluginState(info, info.config as PluginConfig);
      projects.set(info.project, state);
      return state.proxy;
    },
    getExternalFiles(project) {
      const state = projects.get(project);
      if (!state) return [];
      return [...state.lanes.values()].flatMap((lane) => [
        ...lane.project.layout().sourceFiles,
        lane.project.layout().configPath,
      ]);
    },
  };
}

function mapGenerated(
  map: import('@boundaryml/baml-tooling').SegmentMap,
  offset: number,
):
  | { path: string; startUtf8: number; lengthUtf8: number; symbolId: string }
  | undefined {
  const segment = map.segments.find(
    (candidate) =>
      offset >= candidate.genStartUtf16 &&
      offset < candidate.genStartUtf16 + candidate.genLengthUtf16,
  );
  const path = segment && map.sources[segment.sourceFile];
  return segment && path
    ? {
        lengthUtf8: segment.sourceLengthUtf8,
        path,
        startUtf8: segment.sourceStartUtf8,
        symbolId: segment.symbolId,
      }
    : undefined;
}

function isModuleSpecifierPosition(
  typescript: typeof ts,
  sourceFile: ts.SourceFile,
  position: number,
): boolean {
  let found = false;
  const visit = (node: ts.Node): void => {
    if (found || position < node.getFullStart() || position > node.getEnd())
      return;
    if (typescript.isStringLiteral(node)) {
      const parent = node.parent;
      found =
        position >= node.getStart(sourceFile) + 1 &&
        position <= node.getEnd() - 1 &&
        (((typescript.isImportDeclaration(parent) ||
          typescript.isExportDeclaration(parent)) &&
          parent.moduleSpecifier === node) ||
          (typescript.isExternalModuleReference(parent) &&
            parent.expression === node) ||
          (typescript.isCallExpression(parent) &&
            parent.expression.kind === typescript.SyntaxKind.ImportKeyword));
      return;
    }
    typescript.forEachChild(node, visit);
  };
  visit(sourceFile);
  return found;
}

function relativeBamlSpecifier(importer: string, source: string): string {
  const path = relative(dirname(importer), source).replaceAll('\\', '/');
  return path.startsWith('.') ? path : `./${path}`;
}

function bamlImportSpans(
  typescript: typeof ts,
  sourceFile: ts.SourceFile,
  affectedPath?: string,
  specifierText?: string,
): ts.TextSpan[] {
  const spans: ts.TextSpan[] = [];
  for (const statement of sourceFile.statements) {
    if (!typescript.isImportDeclaration(statement)) continue;
    const specifier = statement.moduleSpecifier;
    if (!typescript.isStringLiteral(specifier)) continue;
    if (specifier.text !== 'baml:client' && !specifier.text.endsWith('.baml'))
      continue;
    if (specifierText !== undefined && specifier.text !== specifierText)
      continue;
    if (
      affectedPath &&
      specifier.text !== 'baml:client' &&
      resolveBamlSpecifier(specifier.text, sourceFile.fileName) !==
        canonicalPath(affectedPath)
    )
      continue;
    spans.push({
      length: specifier.getWidth(sourceFile) - 2,
      start: specifier.getStart(sourceFile) + 1,
    });
  }
  return spans;
}

function canonicalPath(path: string): string {
  try {
    return realpathSync(path);
  } catch {
    return resolve(path);
  }
}
function dedupe<T>(values: readonly T[], key: (value: T) => string): T[] {
  const seen = new Set<string>();
  return values.filter((value) => {
    const id = key(value);
    if (seen.has(id)) return false;
    seen.add(id);
    return true;
  });
}

export = init;
