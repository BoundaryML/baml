import { mkdir, mkdtemp, realpath, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import ts from 'typescript';
import { describe, expect, it } from 'vitest';

const nativeBridge = process.env.BAML_TOOLING_BRIDGE_PATH;
const require = createRequire(import.meta.url);
const initPlugin =
  require('@boundaryml/baml-tooling/typescript-plugin') as (options: {
    typescript: typeof ts;
  }) => ts.server.PluginModule;

describe.runIf(nativeBridge)('real TypeScript language service', () => {
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
});
