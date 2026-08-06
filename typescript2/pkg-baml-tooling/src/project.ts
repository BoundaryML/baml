import type { ToolingBackend } from './backend.js';
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
  private constructor() {
    throw new Error('not implemented');
  }

  static async open(options: OpenProjectOptions): Promise<BamlProject> {
    void options;
    throw new Error('not implemented');
  }

  layout(): ProjectLayout {
    throw new Error('not implemented');
  }

  refreshLayout(): ProjectLayout {
    throw new Error('not implemented');
  }

  capabilities(): Capabilities {
    throw new Error('not implemented');
  }

  fingerprint(): string {
    throw new Error('not implemented');
  }

  updateFile(path: string, text: string | null, version?: number): void {
    void path;
    void text;
    void version;
    throw new Error('not implemented');
  }

  check(): CheckResult {
    throw new Error('not implemented');
  }

  resolveModule(
    id: string,
    importer?: string,
  ): { code: string; watchFiles: string[] } {
    void id;
    void importer;
    throw new Error('not implemented');
  }

  resolveDts(
    id: string,
    importer?: string,
  ): { code: string; map: SegmentMap; watchFiles: string[]; stale: boolean } {
    void id;
    void importer;
    throw new Error('not implemented');
  }

  resolveRuntime(): RuntimeModule {
    throw new Error('not implemented');
  }

  mappingsFor(id: string, importer?: string): SegmentMap {
    void id;
    void importer;
    throw new Error('not implemented');
  }

  definitionAt(path: string, offsetUtf8: number, symbolId = ''): Location[] {
    void path;
    void offsetUtf8;
    void symbolId;
    throw new Error('not implemented');
  }

  referencesAt(path: string, offsetUtf8: number, symbolId = ''): Location[] {
    void path;
    void offsetUtf8;
    void symbolId;
    throw new Error('not implemented');
  }

  hoverAt(path: string, offsetUtf8: number, symbolId: string): Hover {
    void path;
    void offsetUtf8;
    void symbolId;
    throw new Error('not implemented');
  }

  completionsAt(
    path = '',
    offsetUtf8 = 0,
    entry = 'baml:client',
  ): CompletionItem[] {
    void path;
    void offsetUtf8;
    void entry;
    throw new Error('not implemented');
  }

  prepareRename(symbolId: string): Location {
    void symbolId;
    throw new Error('not implemented');
  }

  rename(symbolId: string, newName: string): WorkspaceEdit {
    void symbolId;
    void newName;
    throw new Error('not implemented');
  }

  generatedToSource(
    id: string,
    offsetUtf16: number,
    importer?: string,
  ): ReturnType<typeof import('./mapping.js').generatedToSource> {
    void id;
    void offsetUtf16;
    void importer;
    throw new Error('not implemented');
  }

  sourceToGenerated(
    id: string,
    path: string,
    offsetUtf8: number,
    importer?: string,
  ): ReturnType<typeof import('./mapping.js').sourceToGenerated> {
    void id;
    void path;
    void offsetUtf8;
    void importer;
    throw new Error('not implemented');
  }

  sourceText(path: string): string | undefined {
    void path;
    throw new Error('not implemented');
  }

  dispose(): void {
    throw new Error('not implemented');
  }
}

export function discoverProject(
  start: string,
  fileExists: FileAccess['fileExists'],
): string {
  void start;
  void fileExists;
  throw new Error('not implemented');
}

export function discoverBamlFiles(
  root: string,
  readFile?: FileAccess['readFile'],
): Map<string, string> {
  void root;
  void readFile;
  throw new Error('not implemented');
}

export function projectId(root: string): string {
  void root;
  throw new Error('not implemented');
}

export function moduleSpecifier(source: string, importer: string): string {
  void source;
  void importer;
  throw new Error('not implemented');
}
