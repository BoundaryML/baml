import { existsSync } from 'node:fs';
import {
  mkdir,
  mkdtemp,
  readFile,
  realpath,
  rm,
  symlink,
  writeFile,
} from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import ts from 'typescript';
import { describe, expect, it } from 'vitest';
import { ToolingRequest } from '../../pkg-baml-tooling/src/generated/tooling.js';

const nativeBridgePath = resolve(
  import.meta.dirname,
  '../../../baml_language/sdks/typescript/bridge_tooling/baml-tooling-node.js',
);
process.env.BAML_TOOLING_BRIDGE_PATH ??= nativeBridgePath;
const require = createRequire(import.meta.url);
const initPlugin =
  require('@boundaryml/baml-tooling/typescript-plugin') as (options: {
    typescript: typeof ts;
  }) => ts.server.PluginModule;

interface ServiceHarness {
  proxy: ts.LanguageService;
  texts: Map<string, string>;
  versions: Map<string, number>;
  /** Buffers the editor is holding that the TypeScript project does not own
   * and that may never have been written to disk — exactly what tsserver
   * tracks in `ProjectService`, and the only channel through which a new
   * unsaved `.baml` file can reach the compiler. */
  buffers: Map<string, string>;
  /** ScriptInfo objects tsserver retains after a file is no longer open. */
  retainedScriptInfos: Set<string>;
  root: string;
  externalFiles(): string[];
  ready(): Promise<void>;
}

async function createService(
  files: Readonly<Record<string, string>>,
  consumers: readonly string[],
  options: {
    debug?: boolean;
    log?: (message: string) => void;
    prepare?: (root: string, base: string) => Promise<void>;
    rootSuffix?: string;
  } = {},
): Promise<ServiceHarness> {
  const base = await realpath(await mkdtemp(join(tmpdir(), 'baml-ls-')));
  const root = options.rootSuffix ? join(base, options.rootSuffix) : base;
  await mkdir(root, { recursive: true });
  await writeFile(join(root, 'baml.toml'), '');
  const texts = new Map<string, string>();
  const versions = new Map<string, number>();
  const retainedScriptInfos = new Set<string>();
  for (const [name, text] of Object.entries(files)) {
    const path = join(root, name);
    await mkdir(resolve(path, '..'), { recursive: true });
    await writeFile(path, text);
    const canonicalPath = await realpath(path);
    texts.set(canonicalPath, text);
    versions.set(canonicalPath, 1);
    if (canonicalPath.endsWith('.baml')) retainedScriptInfos.add(canonicalPath);
  }
  await options.prepare?.(root, base);
  let projectVersion = 1;
  const host: ts.LanguageServiceHost = {
    fileExists: ts.sys.fileExists,
    getCompilationSettings: () => ({
      allowNonTsExtensions: true,
      module: ts.ModuleKind.ESNext,
      moduleResolution: ts.ModuleResolutionKind.Bundler,
      strict: true,
      target: ts.ScriptTarget.ES2022,
    }),
    getCurrentDirectory: () => root,
    getDefaultLibFileName: ts.getDefaultLibFilePath,
    getProjectVersion: () => String(projectVersion),
    getScriptFileNames: () => consumers.map((name) => join(root, name)),
    getScriptSnapshot: (fileName) => {
      const text = texts.get(fileName) ?? ts.sys.readFile(fileName);
      return text === undefined
        ? undefined
        : ts.ScriptSnapshot.fromString(text);
    },
    getScriptVersion: (fileName) => String(versions.get(fileName) ?? 0),
    readFile: (fileName) => texts.get(fileName) ?? ts.sys.readFile(fileName),
  };
  const languageService = ts.createLanguageService(host);
  // tsserver's open-file registry. Modelled as its own map, deliberately not
  // reachable through `host.getScriptSnapshot`/`fileExists`/`readDirectory`:
  // a `.baml` buffer is not a script of the TypeScript project, and the
  // process filesystem cannot see it until it is saved.
  const buffers = new Map<string, string>();
  const bufferVersions = new Map<string, number>();
  const project = {
    getCurrentDirectory: () => root,
    projectService: {
      getScriptInfo: (name: string) =>
        buffers.has(name) || retainedScriptInfos.has(name)
          ? {
              fileName: name,
              getLatestVersion: () =>
                String(bufferVersions.get(name) ?? versions.get(name) ?? 1),
              getSnapshot: () =>
                ts.ScriptSnapshot.fromString(
                  buffers.get(name) ?? texts.get(name) ?? '',
                ),
              isScriptOpen: () => buffers.has(name),
            }
          : undefined,
      logger: { info: options.log ?? (() => undefined) },
      openFiles: buffers,
    },
    refreshDiagnostics: () => {
      projectVersion++;
    },
  };
  const plugin = initPlugin({ typescript: ts });
  const proxy = plugin.create({
    config: { debug: options.debug, root },
    languageService,
    languageServiceHost: host,
    project,
  } as unknown as ts.server.PluginCreateInfo);
  return {
    buffers,
    externalFiles: () => plugin.getExternalFiles?.(project as never, 0) ?? [],
    proxy,
    async ready() {
      for (let attempt = 0; attempt < 100; attempt++) {
        const external = plugin.getExternalFiles?.(project as never, 0) ?? [];
        if (external.length > 0) return;
        await new Promise((resolveWait) => setTimeout(resolveWait, 10));
      }
      throw new Error('BAML tooling did not open the project');
    },
    retainedScriptInfos,
    root,
    texts,
    versions,
  };
}

async function waitFor(
  predicate: () => boolean,
  message: string,
): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt++) {
    if (predicate()) return;
    await new Promise((resolveWait) => setTimeout(resolveWait, 25));
  }
  throw new Error(`timed out waiting for ${message}`);
}

describe('real TypeScript language service', () => {
  it('maps all cross-language navigation to physical BAML source', async () => {
    const root = await mkdtemp(join(tmpdir(), 'baml-language-service-'));
    const source = join(root, 'baml_src/main.baml');
    const consumer = join(root, 'consumer.ts');
    await mkdir(join(root, 'baml_src'));
    await writeFile(join(root, 'baml.toml'), '');
    const baml =
      '/// Person documentation\nclass Person {\n  name string\n}\n/// Greeting documentation\nfunction Greet(p: Person) -> string {\n  "hi"\n}\n';
    const typescript =
      "import { b, Person, Person$stream } from './baml_src/main.baml';\nimport { b as client } from 'baml:client';\nconst p: Person = { name: 'Ada' };\ntype Stream = Person$stream;\nexport const name = p.name;\nexport const greeting = b.Greet(p) + client.Greet(p);\n";
    await writeFile(source, baml);
    await writeFile(consumer, typescript);
    const canonicalSource = await realpath(source);
    const texts = new Map([
      [consumer, typescript],
      [canonicalSource, baml],
    ]);
    const versions = new Map([
      [consumer, 1],
      [canonicalSource, 1],
    ]);
    let projectVersion = 1;
    const host: ts.LanguageServiceHost = {
      fileExists: ts.sys.fileExists,
      getCompilationSettings: () => ({
        allowNonTsExtensions: true,
        module: ts.ModuleKind.ESNext,
        moduleResolution: ts.ModuleResolutionKind.Bundler,
        strict: true,
        target: ts.ScriptTarget.ES2022,
      }),
      getCurrentDirectory: () => root,
      getDefaultLibFileName: ts.getDefaultLibFilePath,
      getProjectVersion: () => String(projectVersion),
      getScriptFileNames: () => [consumer],
      getScriptSnapshot: (fileName) => {
        const text = texts.get(fileName) ?? ts.sys.readFile(fileName);
        return text === undefined
          ? undefined
          : ts.ScriptSnapshot.fromString(text);
      },
      getScriptVersion: (fileName) => String(versions.get(fileName) ?? 0),
      readFile: (fileName) => texts.get(fileName) ?? ts.sys.readFile(fileName),
    };
    const languageService = ts.createLanguageService(host);
    const project = {
      getCurrentDirectory: () => root,
      projectService: { logger: { info: () => undefined } },
      refreshDiagnostics: () => {
        projectVersion++;
      },
    };
    const plugin = initPlugin({ typescript: ts });
    const proxy = plugin.create({
      config: { root },
      languageService,
      languageServiceHost: host,
      project,
    } as unknown as ts.server.PluginCreateInfo);

    for (let attempt = 0; attempt < 100; attempt++) {
      if (
        plugin.getExternalFiles?.(project as never, 0).includes(canonicalSource)
      )
        break;
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
    const greetPosition = typescript.lastIndexOf('Greet');
    const definitions = proxy.getDefinitionAtPosition(consumer, greetPosition);
    expect(definitions?.[0]?.fileName).toBe(canonicalSource);
    expect(
      definitions?.every((item) => !item.fileName.includes('__virtual__')),
    ).toBe(true);
    for (const position of [
      typescript.indexOf('Person ='),
      typescript.lastIndexOf('Person$stream'),
      typescript.indexOf('p.name') + 2,
      typescript.indexOf('client.Greet') + 7,
    ]) {
      const tokenDefinitions = proxy.getDefinitionAtPosition(
        consumer,
        position,
      );
      expect(tokenDefinitions?.[0]?.fileName).toBe(canonicalSource);
      expect(
        tokenDefinitions?.every(
          (item) => !item.fileName.includes('__virtual__'),
        ),
      ).toBe(true);
    }
    expect(proxy.getReferencesAtPosition(consumer, greetPosition)).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ fileName: consumer }),
        expect.objectContaining({ fileName: canonicalSource }),
      ]),
    );
    expect(proxy.getRenameInfo(consumer, greetPosition).canRename).toBe(true);
    expect(
      proxy.findRenameLocations(consumer, greetPosition, false, false, {}),
    ).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ fileName: consumer }),
        expect.objectContaining({ fileName: canonicalSource }),
      ]),
    );
    expect(
      proxy.getQuickInfoAtPosition(consumer, greetPosition)?.documentation,
    ).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          text: expect.stringContaining('Greeting documentation'),
        }),
      ]),
    );
    const memberPosition = typescript.indexOf('b.Greet') + 2;
    expect(
      proxy
        .getCompletionsAtPosition(consumer, memberPosition, {})
        ?.entries.map((entry) => entry.name),
    ).toContain('Greet');
    expect(
      proxy
        .getCompletionsAtPosition(consumer, memberPosition, {})
        ?.entries.map((entry) => entry.name),
    ).not.toContain('baml:client');
    expect(
      proxy
        .getCompletionsAtPosition(
          consumer,
          typescript.indexOf('./baml_src/main.baml') + 2,
          {},
        )
        ?.entries.map((entry) => entry.name),
    ).toContain('baml:client');

    texts.set(canonicalSource, 'class Person {');
    versions.set(canonicalSource, 2);
    projectVersion++;
    const diagnostics = proxy.getSemanticDiagnostics(consumer);
    expect(diagnostics.some((item) => item.code === 91002)).toBe(true);
    expect(diagnostics.some((item) => item.code === 2339)).toBe(false);
    texts.set(canonicalSource, baml);
    versions.set(canonicalSource, 3);
    projectVersion++;
    expect(
      proxy
        .getSemanticDiagnostics(consumer)
        .some((item) => item.code === 91002),
    ).toBe(false);
  });

  it('keys virtual declaration files by the importer-resolved source', async () => {
    // Two consumers in different directories both import `./schema.baml`;
    // each names a different physical source, so each must get its own
    // virtual declaration file — sharing the first importer's virtual file
    // would hand the second consumer the wrong types and navigation.
    const alphaBaml = 'class Alpha { x int }\n';
    const betaBaml = 'class Beta { y string }\n';
    const consumerAText =
      "import { Alpha } from './schema.baml';\nexport const value: Alpha = { x: 1 };\n";
    const consumerBText =
      "import { Beta } from './schema.baml';\nexport const value: Beta = { y: 's' };\n";
    const service = await createService(
      {
        'a/consumer.ts': consumerAText,
        'a/schema.baml': alphaBaml,
        'b/consumer.ts': consumerBText,
        'b/schema.baml': betaBaml,
      },
      ['a/consumer.ts', 'b/consumer.ts'],
    );
    await service.ready();
    const consumerA = join(service.root, 'a/consumer.ts');
    const consumerB = join(service.root, 'b/consumer.ts');
    const alphaDefinitions = service.proxy.getDefinitionAtPosition(
      consumerA,
      consumerAText.indexOf('Alpha ='),
    );
    expect(alphaDefinitions?.[0]?.fileName).toBe(
      join(service.root, 'a/schema.baml'),
    );
    const betaDefinitions = service.proxy.getDefinitionAtPosition(
      consumerB,
      consumerBText.indexOf('Beta ='),
    );
    expect(betaDefinitions?.[0]?.fileName).toBe(
      join(service.root, 'b/schema.baml'),
    );
    expect(
      service.proxy
        .getSemanticDiagnostics(consumerB)
        .filter((item) => item.category === ts.DiagnosticCategory.Error),
    ).toEqual([]);
  });

  it('discovers added and removes deleted .baml files in the overlay', async () => {
    const recorder = await mkdtemp(join(tmpdir(), 'baml-bridge-recorder-'));
    const requestLog = join(recorder, 'requests.log');
    const recordingBridge = join(recorder, 'bridge.cjs');
    await writeFile(
      recordingBridge,
      `const { appendFileSync } = require('node:fs');
const native = require(${JSON.stringify(nativeBridgePath)});
exports.BamlToolingBridge = class {
  constructor() { this.inner = new native.BamlToolingBridge(); }
  dispatch(request) {
    appendFileSync(${JSON.stringify(requestLog)}, Buffer.from(request).toString('base64') + '\\n');
    return this.inner.dispatch(request);
  }
};
`,
    );
    const previousBridge = process.env.BAML_TOOLING_BRIDGE_PATH;
    process.env.BAML_TOOLING_BRIDGE_PATH = recordingBridge;
    try {
      const mainBaml =
        'class Person { name string }\nfunction Removed(p: Person) -> string {\n  "gone"\n}\n';
      const addedBaml = 'class Added { note string }\n';
      const consumerText =
        "import { b } from 'baml:client';\nimport { Person } from './main.baml';\nimport './';\nexport const person: Person = { name: 'Ada' };\nexport const removed = b.Removed;\n";
      const service = await createService(
        { 'consumer.ts': consumerText, 'main.baml': mainBaml },
        ['consumer.ts'],
      );
      await service.ready();
      const consumer = join(service.root, 'consumer.ts');
      const main = join(service.root, 'main.baml');
      const specifierPosition = consumerText.indexOf("'./'") + 2;
      const specifierNames = () =>
        service.proxy
          .getCompletionsAtPosition(consumer, specifierPosition, {})
          ?.entries.map((entry) => entry.name) ?? [];
      const clientExports = () =>
        service.proxy
          .getCompletionsAtPosition(
            consumer,
            consumerText.indexOf('b.Removed') + 2,
            {},
          )
          ?.entries.map((entry) => entry.name) ?? [];
      expect(specifierNames()).toContain('./main.baml');
      expect(clientExports()).toContain('Removed');

      // A .baml file created after the project opened is discovered and
      // becomes importable without a tsserver restart.
      const added = join(service.root, 'added.baml');
      await writeFile(added, addedBaml);
      const canonicalAdded = await realpath(added);
      service.texts.set(canonicalAdded, addedBaml);
      service.versions.set(canonicalAdded, 1);
      expect(specifierNames()).toContain('./added.baml');

      // Real tsserver retains ScriptInfo after the editor closes a file. The
      // object remains here with isScriptOpen() false after disk deletion;
      // treating object existence as openness reproduces the phantom-symbol
      // bug and makes every assertion below fail.
      expect(service.retainedScriptInfos.has(main)).toBe(true);
      expect(service.buffers.has(main)).toBe(false);
      await rm(main);
      service.texts.delete(main);
      expect(specifierNames()).not.toContain('./main.baml');
      expect(specifierNames()).toContain('./added.baml');
      expect(clientExports()).not.toContain('Removed');
      expect(
        service.proxy
          .getSemanticDiagnostics(consumer)
          .some((item) => item.category === ts.DiagnosticCategory.Error),
      ).toBe(true);

      const requests = (await readFile(requestLog, 'utf8'))
        .trim()
        .split('\n')
        .map((line) => ToolingRequest.decode(Buffer.from(line, 'base64')));
      expect(
        requests.some(
          (request) =>
            request.request?.$case === 'update' &&
            request.request.update.remove &&
            request.request.update.file?.path === main,
        ),
      ).toBe(true);
    } finally {
      if (previousBridge === undefined)
        delete process.env.BAML_TOOLING_BRIDGE_PATH;
      else process.env.BAML_TOOLING_BRIDGE_PATH = previousBridge;
      await rm(recorder, { force: true, recursive: true });
    }
  });

  it('discovers a host-only .baml buffer with no disk write and no import', async () => {
    const consumerText =
      "import { b } from 'baml:client';\nimport './';\nexport const client = b.Describe;\n";
    const service = await createService(
      {
        'consumer.ts': consumerText,
        'main.baml':
          'class Person { name string }\nfunction Greet(p: Person) -> string {\n  "hi"\n}\n',
      },
      ['consumer.ts'],
    );
    await service.ready();
    const consumer = join(service.root, 'consumer.ts');
    const specifierPosition = consumerText.indexOf("'./'") + 2;
    const specifierNames = () =>
      service.proxy
        .getCompletionsAtPosition(consumer, specifierPosition, {})
        ?.entries.map((entry) => entry.name) ?? [];
    // Member completions on the `baml:client` aggregate: what the compiler
    // says the project exports right now.
    const clientExports = () =>
      service.proxy
        .getCompletionsAtPosition(
          consumer,
          consumerText.indexOf('b.Describe') + 2,
          {},
        )
        ?.entries.map((entry) => entry.name) ?? [];
    // Baseline: the aggregate client is live and does not yet know the buffer.
    await waitFor(
      () => clientExports().includes('Greet'),
      'the `baml:client` aggregate to resolve',
    );
    expect(clientExports()).not.toContain('Describe');
    expect(specifierNames()).not.toContain('./unsaved.baml');

    // The editor opens a brand-new `.baml` buffer. It is never written to
    // disk, so no directory stamp moves, and nothing imports it, so no
    // resolved import target registers it as a candidate. Only the host's own
    // listing can reveal it.
    const unsaved = join(service.root, 'unsaved.baml');
    service.buffers.set(
      unsaved,
      'class Unsaved { note string }\nfunction Describe(u: Unsaved) -> string {\n  "hi"\n}\n',
    );
    expect(existsSync(unsaved)).toBe(false);

    // Compiler layout, `baml:client` declarations, and completion candidates
    // all pick the buffer up — with no save, no import, and no restart.
    await waitFor(
      () => specifierNames().includes('./unsaved.baml'),
      'the unsaved buffer to join the discovered file set',
    );
    expect(service.externalFiles()).toContain(unsaved);
    await waitFor(
      () => clientExports().includes('Describe'),
      'the `baml:client` aggregate to export the unsaved buffer',
    );
    expect(clientExports()).toContain('Greet');
    expect(existsSync(unsaved)).toBe(false);

    // It must also stay: the deletion sweep runs on every request and reads
    // existence from the host, so a buffer with no disk entry is not mistaken
    // for a file that was deleted.
    for (let request = 0; request < 3; request++)
      service.proxy.getSemanticDiagnostics(consumer);
    expect(specifierNames()).toContain('./unsaved.baml');
    expect(clientExports()).toContain('Describe');
    expect(clientExports()).toContain('Greet');

    // Closing the buffer without ever saving withdraws it again: the host
    // listing is compared on every poll, so it moves in both directions.
    service.buffers.delete(unsaved);
    await waitFor(
      () => !specifierNames().includes('./unsaved.baml'),
      'the closed buffer to leave the discovered file set',
    );
  });

  it('does not re-walk the project tree on every request', async () => {
    const logs: string[] = [];
    const service = await createService(
      {
        'consumer.ts':
          "import { b } from './main.baml';\nexport const client = b;\n",
        'main.baml': 'class Person { name string }\n',
      },
      ['consumer.ts'],
      { debug: true, log: (message) => logs.push(message) },
    );
    await service.ready();
    const consumer = join(service.root, 'consumer.ts');
    service.proxy.getSemanticDiagnostics(consumer);
    const walksAfterWarmup = logs.filter((message) =>
      message.includes('rediscovered'),
    ).length;
    // Keystroke-paced requests with an unchanged tree must not pay for
    // another recursive discovery walk.
    for (let request = 0; request < 5; request++) {
      service.proxy.getSemanticDiagnostics(consumer);
      service.proxy.getCompletionsAtPosition(consumer, 10, {});
      service.proxy.getQuickInfoAtPosition(consumer, 10);
    }
    expect(
      logs.filter((message) => message.includes('rediscovered')).length,
    ).toBe(walksAfterWarmup);
  });

  it('serves .baml imports from a dependency inside node_modules', async () => {
    const dependencyBaml =
      '/// Widget docs\nclass Widget { label string }\nfunction Describe(widget: Widget) -> string { "dep" }\n';
    const consumerText =
      "import { b, Widget } from 'baml-dep/baml_src/widget.baml';\ndeclare const widget: Widget;\nexport const description = b.Describe(widget);\n";
    const service = await createService(
      {
        'consumer.ts': consumerText,
        'main.baml': 'class Local { count int }\n',
        'node_modules/baml-dep/baml_src/widget.baml': dependencyBaml,
        'node_modules/baml-dep/baml.toml': '',
        'node_modules/baml-dep/package.json': JSON.stringify({
          name: 'baml-dep',
          version: '1.0.0',
        }),
      },
      ['consumer.ts'],
    );
    await service.ready();
    const consumer = join(service.root, 'consumer.ts');
    const widgetSource = join(
      service.root,
      'node_modules/baml-dep/baml_src/widget.baml',
    );
    // Force module resolution, then wait for the dependency's own compiler
    // session (owned by the baml.toml inside node_modules) to open.
    service.proxy.getSemanticDiagnostics(consumer);
    await waitFor(
      () => service.externalFiles().includes(widgetSource),
      'the dependency project session to open',
    );
    const position = consumerText.indexOf('widget: Widget') + 8;
    const definitions = service.proxy.getDefinitionAtPosition(
      consumer,
      position,
    );
    expect(definitions?.[0]?.fileName).toBe(widgetSource);
    expect(
      definitions?.every((item) => !item.fileName.includes('__virtual__')),
    ).toBe(true);
    expect(
      service.proxy.getQuickInfoAtPosition(consumer, position)?.documentation,
    ).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          text: expect.stringContaining('Widget docs'),
        }),
      ]),
    );
    expect(
      service.proxy
        .getSemanticDiagnostics(consumer)
        .filter((item) => item.category === ts.DiagnosticCategory.Error),
    ).toEqual([]);
  });

  it('serves .baml imports across pnpm-style workspace symlinks', async () => {
    const sharedBaml =
      '/// Shared docs\nclass Shared { note string }\nfunction Summarize(shared: Shared) -> string { "shared" }\n';
    const consumerText =
      "import { b as app } from './app.baml';\nimport { b, Shared } from '@shared/baml/baml_src/shared.baml';\ndeclare const value: Shared;\nexport const result = [app.Check({ flag: true }), b.Summarize(value)] as const;\n";
    const service = await createService(
      {
        'app.baml':
          'class AppLocal { flag bool }\nfunction Check(app: AppLocal) -> string { "app" }\n',
        'consumer.ts': consumerText,
      },
      ['consumer.ts'],
      {
        prepare: async (root, base) => {
          const shared = join(base, 'packages', 'shared');
          await mkdir(join(shared, 'baml_src'), { recursive: true });
          await writeFile(
            join(shared, 'package.json'),
            JSON.stringify({ name: '@shared/baml', version: '0.0.0' }),
          );
          await writeFile(join(shared, 'baml.toml'), '');
          await writeFile(join(shared, 'baml_src', 'shared.baml'), sharedBaml);
          // pnpm links workspace packages: node_modules/@shared/baml is a
          // symlink to the sibling package.
          await mkdir(join(root, 'node_modules', '@shared'), {
            recursive: true,
          });
          await symlink(
            shared,
            join(root, 'node_modules', '@shared', 'baml'),
            'dir',
          );
        },
        rootSuffix: join('packages', 'app'),
      },
    );
    await service.ready();
    const consumer = join(service.root, 'consumer.ts');
    // The physical source behind the symlink, owned by the sibling's own
    // baml.toml — never the node_modules symlink path.
    const sharedSource = join(
      service.root,
      '..',
      'shared',
      'baml_src',
      'shared.baml',
    );
    service.proxy.getSemanticDiagnostics(consumer);
    await waitFor(
      () => service.externalFiles().includes(sharedSource),
      'the workspace sibling project session to open',
    );
    const position = consumerText.indexOf('value: Shared') + 7;
    const definitions = service.proxy.getDefinitionAtPosition(
      consumer,
      position,
    );
    expect(definitions?.[0]?.fileName).toBe(sharedSource);
    expect(
      definitions?.every((item) => !item.fileName.includes('__virtual__')),
    ).toBe(true);
    expect(
      service.proxy
        .getSemanticDiagnostics(consumer)
        .filter((item) => item.category === ts.DiagnosticCategory.Error),
    ).toEqual([]);
  });

  it('reports unresolvable bare .baml imports on the import specifier', async () => {
    const consumerText =
      "import { b } from 'missing-dep/baml_src/widget.baml';\nexport const client = b;\n";
    const service = await createService(
      {
        'consumer.ts': consumerText,
        'main.baml': 'class Local { count int }\n',
      },
      ['consumer.ts'],
    );
    await service.ready();
    const consumer = join(service.root, 'consumer.ts');
    const diagnostics = service.proxy.getSemanticDiagnostics(consumer);
    // Loud failure: the editor must show why the import has no types rather
    // than silently rendering an empty declaration.
    const reported = diagnostics.filter((item) => item.code === 91004);
    expect(reported).toHaveLength(1);
    expect(String(reported[0]?.messageText)).toContain(
      'missing-dep/baml_src/widget.baml',
    );
    const start = reported[0]?.start ?? 0;
    expect(consumerText.slice(start, start + (reported[0]?.length ?? 0))).toBe(
      'missing-dep/baml_src/widget.baml',
    );
  });

  it('keeps serving after raced config reopens', async () => {
    const mainBaml =
      'class Person { name string }\nfunction Greet(p: Person) -> string { "hi" }\n';
    const consumerText =
      "import { b } from './main.baml';\nconst p = { name: 'Ada' };\nexport const greeting = b.Greet(p);\n";
    const service = await createService(
      { 'consumer.ts': consumerText, 'main.baml': mainBaml },
      ['consumer.ts'],
    );
    await service.ready();
    const consumer = join(service.root, 'consumer.ts');
    const config = join(service.root, 'baml.toml');
    // Two config revisions in the same tick race two session loads; the
    // superseded load must dispose its native session instead of leaking it,
    // and the surviving session must keep serving navigation.
    service.versions.set(config, 2);
    service.proxy.getSemanticDiagnostics(consumer);
    service.versions.set(config, 3);
    service.proxy.getSemanticDiagnostics(consumer);
    const main = join(service.root, 'main.baml');
    await waitFor(() => {
      const definitions = service.proxy.getDefinitionAtPosition(
        consumer,
        consumerText.indexOf('Greet'),
      );
      return definitions?.some((item) => item.fileName === main) ?? false;
    }, 'raced reopens to settle');
  });
});
