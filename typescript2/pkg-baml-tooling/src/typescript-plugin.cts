import { realpathSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { buildLineIndex, utf8OffsetToUtf16 } from '@boundaryml/baml-tooling';
import type ts from 'typescript/lib/tsserverlibrary';

interface PluginConfig {
  target?: 'node' | 'web';
  debug?: boolean;
  root?: string;
}

interface VirtualFile {
  fileName: string;
  specifier: string;
  importer: string;
  declaration: string;
  version: number;
  map?: import('@boundaryml/baml-tooling').SegmentMap;
  scriptInfo?: ts.server.ScriptInfo;
  stale: boolean;
}

const INITIALIZING = 'declare const b: never; export { b };\n';
const DIAGNOSTIC_BASE = 91000;

function init({
  typescript,
}: {
  typescript: typeof ts;
}): ts.server.PluginModule {
  const projects = new WeakMap<ts.server.Project, PluginState>();

  class PluginState {
    readonly virtualFiles = new Map<string, VirtualFile>();
    readonly proxy: ts.LanguageService;
    project?: import('@boundaryml/baml-tooling').BamlProject;
    error?: Error;
    revision = 0;
    readonly sourceVersions = new Map<string, string>();
    configVersion = '';

    constructor(
      readonly info: ts.server.PluginCreateInfo,
      readonly config: PluginConfig,
    ) {
      this.decorateHost();
      this.proxy = this.decorateLanguageService();
      void this.open();
    }

    async open(): Promise<void> {
      const requestedRevision = ++this.revision;
      try {
        const { loadProject } = await import('@boundaryml/baml-tooling');
        const project = await loadProject({
          backend: 'auto',
          cwd: this.config.root ?? this.info.project.getCurrentDirectory(),
          fileExists: (path) =>
            this.info.languageServiceHost.fileExists?.(path) ?? false,
          readFile: (path) =>
            this.snapshotText(path) ??
            this.info.languageServiceHost.readFile?.(path),
          target: this.config.target ?? 'node',
        });
        if (requestedRevision !== this.revision) return;
        this.project = project;
        this.configVersion =
          this.info.languageServiceHost.getScriptVersion?.(
            project.layout().configPath,
          ) ?? '0';
        for (const source of project.layout().sourceFiles)
          this.sourceVersions.set(
            source,
            this.info.languageServiceHost.getScriptVersion?.(source) ?? '0',
          );
        for (const virtual of this.virtualFiles.values())
          this.refreshVirtual(virtual);
        this.invalidate();
      } catch (error) {
        this.error = error instanceof Error ? error : new Error(String(error));
        this.log(this.error.message);
        this.invalidate();
      }
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
        if (!this.project?.capabilities().features.includes('rename.v1'))
          return {
            canRename: false,
            localizedErrorMessage:
              'rename across the BAML boundary requires a newer BAML compiler bridge',
          };
        try {
          const location = this.project.prepareRename(mapped.symbolId);
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
        if (!mapped || !this.project) return original;
        try {
          this.project.prepareRename(mapped.symbolId);
          const compiler = this.project
            .referencesAt('', 0, mapped.symbolId)
            .concat(this.project.definitionAt('', 0, mapped.symbolId))
            .map((location) => ({
              fileName: location.path,
              textSpan: this.toTextSpan(location),
            }));
          return dedupe(
            this.translateRename(original).concat(compiler),
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
        if (!this.project) {
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
        const check = this.project.check();
        for (const diagnostic of check.diagnostics) {
          if (diagnostic.location?.path === fileName)
            diagnostics.push({
              category:
                diagnostic.severity === 'error'
                  ? typescript.DiagnosticCategory.Error
                  : typescript.DiagnosticCategory.Warning,
              code: DIAGNOSTIC_BASE + 2,
              file: languageService.getProgram()?.getSourceFile(serviceFile),
              length: this.toTextSpan(diagnostic.location).length,
              messageText: diagnostic.message,
              start: this.toTextSpan(diagnostic.location).start,
            });
          else {
            const sourceFile = languageService
              .getProgram()
              ?.getSourceFile(serviceFile);
            if (sourceFile)
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
        for (const source of this.project.layout().sourceFiles) {
          const sidecar = this.info.languageServiceHost.readFile?.(
            `${source}.d.ts`,
          );
          const fingerprint =
            sidecar &&
            /^\/\/ baml-fingerprint: ([a-f0-9]+)$/m.exec(sidecar)?.[1];
          const sourceFile = languageService
            .getProgram()
            ?.getSourceFile(serviceFile);
          if (
            sidecar &&
            fingerprint !== this.project.fingerprint() &&
            sourceFile
          )
            for (const span of bamlImportSpans(typescript, sourceFile, source))
              diagnostics.push({
                category: typescript.DiagnosticCategory.Warning,
                code: DIAGNOSTIC_BASE + 3,
                file: sourceFile,
                length: span.length,
                messageText: `stale BAML sidecar for ${source} — run baml-ts-gen`,
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
          this.project &&
          sourceFile &&
          isModuleSpecifierPosition(typescript, sourceFile, position)
        ) {
          const specifiers = [
            'baml:client',
            ...this.project
              .layout()
              .sourceFiles.map((source) =>
                relativeBamlSpecifier(fileName, source),
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
        if (!detail || !mapped || !this.project) return detail;
        try {
          const hover = this.project.hoverAt('', 0, mapped.symbolId);
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
        if (!info || !mapped || !this.project) return info;
        try {
          const hover = this.project.hoverAt('', 0, mapped.symbolId);
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
      const hash = simpleHash(`${root}\0${specifier}`);
      const base =
        specifier === 'baml:client'
          ? 'client'
          : (specifier
              .split(/[\\/]/)
              .pop()
              ?.replace(/[^a-zA-Z0-9_.-]/g, '_') ?? 'module.baml');
      const fileName = `${root}/.baml/__virtual__/p_${hash}/${base}.d.ts`;
      let virtual = this.virtualFiles.get(fileName);
      if (!virtual) {
        virtual = {
          declaration: INITIALIZING,
          fileName,
          importer,
          specifier,
          stale: false,
          version: 0,
        };
        this.virtualFiles.set(fileName, virtual);
        this.ensureVirtualScriptInfo(virtual);
        if (this.project) this.refreshVirtual(virtual);
      }
      return virtual;
    }

    refreshVirtual(virtual: VirtualFile): void {
      if (!this.project) return;
      const previous = virtual.declaration;
      try {
        const artifact = this.project.resolveDts(
          virtual.specifier,
          virtual.importer,
        );
        virtual.declaration = artifact.code;
        virtual.map = artifact.map;
        virtual.stale = artifact.stale;
      } catch {
        virtual.declaration = INITIALIZING;
        virtual.map = undefined;
        virtual.stale = true;
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
      for (const definition of original) {
        const virtual = this.virtualFiles.get(definition.fileName);
        const mapped =
          virtual?.map && mapGenerated(virtual.map, definition.textSpan.start);
        if (mapped) symbolId = mapped.symbolId;
        else if (!virtual) output.push(definition);
      }
      symbolId ??= this.symbolAt(fileName, position)?.symbolId;
      if (symbolId && this.project)
        for (const location of this.project.definitionAt('', 0, symbolId))
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
      const output = this.translateRename(original).map((entry) => ({
        ...entry,
        isDefinition: false,
        isWriteAccess: false,
      }));
      if (mapped && this.project)
        for (const location of this.project
          .referencesAt('', 0, mapped.symbolId)
          .concat(this.project.definitionAt('', 0, mapped.symbolId)))
          output.push({
            fileName: location.path,
            isDefinition: false,
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
    ): T[] {
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
        }
      }
      return output;
    }

    symbolAt(
      fileName: string,
      position: number,
    ): { symbolId: string } | undefined {
      const direct = this.virtualFiles.get(fileName);
      if (direct?.map) return mapGenerated(direct.map, position);
      const definitions =
        this.info.languageService.getDefinitionAtPosition(
          this.serviceFileName(fileName),
          position,
        ) ?? [];
      for (const definition of definitions) {
        const virtual = this.virtualFiles.get(definition.fileName);
        if (!virtual?.map) continue;
        const mapped = mapGenerated(virtual.map, definition.textSpan.start);
        if (mapped) return mapped;
      }
      return undefined;
    }

    toTextSpan(location: {
      path: string;
      startUtf8: number;
      lengthUtf8: number;
    }): ts.TextSpan {
      const text =
        this.snapshotText(location.path) ??
        this.project?.sourceText(location.path) ??
        '';
      const index = buildLineIndex(text);
      const start = utf8OffsetToUtf16(index, location.startUtf8);
      const end = utf8OffsetToUtf16(
        index,
        location.startUtf8 + location.lengthUtf8,
      );
      return { length: end - start, start };
    }

    snapshotText(path: string): string | undefined {
      return this.info.languageServiceHost
        .getScriptSnapshot(path)
        ?.getText(
          0,
          this.info.languageServiceHost.getScriptSnapshot(path)?.getLength() ??
            0,
        );
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
      if (!this.project) return;
      const configPath = this.project.layout().configPath;
      const configVersion =
        this.info.languageServiceHost.getScriptVersion?.(configPath) ?? '0';
      if (configVersion !== this.configVersion) {
        this.configVersion = configVersion;
        void this.open();
        return;
      }
      let changed = false;
      for (const source of this.project.layout().sourceFiles) {
        const version =
          this.info.languageServiceHost.getScriptVersion?.(source) ?? '0';
        if (this.sourceVersions.get(source) === version) continue;
        const snapshot = this.snapshotText(source);
        if (snapshot !== undefined) this.project.updateFile(source, snapshot);
        this.sourceVersions.set(source, version);
        changed = true;
      }
      if (changed)
        for (const virtual of this.virtualFiles.values())
          this.refreshVirtual(virtual);
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
      return state?.project
        ? [
            ...state.project.layout().sourceFiles,
            state.project.layout().configPath,
          ]
        : [];
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

function simpleHash(value: string): string {
  let hash = 2166136261;
  for (const char of value) {
    hash ^= char.charCodeAt(0);
    hash = Math.imul(hash, 16777619);
  }
  return (hash >>> 0).toString(16);
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
): ts.TextSpan[] {
  const spans: ts.TextSpan[] = [];
  for (const statement of sourceFile.statements) {
    if (!typescript.isImportDeclaration(statement)) continue;
    const specifier = statement.moduleSpecifier;
    if (!typescript.isStringLiteral(specifier)) continue;
    if (specifier.text !== 'baml:client' && !specifier.text.endsWith('.baml'))
      continue;
    if (
      affectedPath &&
      specifier.text !== 'baml:client' &&
      canonicalPath(resolve(dirname(sourceFile.fileName), specifier.text)) !==
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
