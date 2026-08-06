import { mkdir, mkdtemp, realpath, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import ts from 'typescript';
import { describe, expect, it } from 'vitest';

const require = createRequire(import.meta.url);
const init = require('../dist/typescript-plugin.cjs') as (options: {
  typescript: typeof ts;
}) => ts.server.PluginModule;

describe('tsserver plugin entrypoint', () => {
  it('exposes the standard create and external-files hooks', () => {
    const plugin = init({ typescript: ts });
    expect(plugin.create).toBeTypeOf('function');
    expect(plugin.getExternalFiles).toBeTypeOf('function');
  });

  it('resolves BAML imports to stable in-memory declaration files', () => {
    const plugin = init({ typescript: ts });
    const snapshots = new Map([
      [
        '/p/index.ts',
        ts.ScriptSnapshot.fromString("import { b } from './x.baml'\n"),
      ],
    ]);
    const host = {
      fileExists: () => false,
      getScriptKind: () => ts.ScriptKind.TS,
      getScriptSnapshot: (fileName: string) => snapshots.get(fileName),
      getScriptVersion: () => '1',
      readFile: () => undefined,
      resolveModuleNameLiterals: (
        literals: readonly ts.StringLiteralLike[],
        _containingFile?: string,
      ) => literals.map(() => ({ resolvedModule: undefined })),
    };
    const languageService = {
      getDefinitionAtPosition: () => undefined,
      getSemanticDiagnostics: () => [],
    };
    plugin.create({
      config: { root: '/p' },
      languageService,
      languageServiceHost: host,
      project: {
        getCurrentDirectory: () => '/p',
        projectService: { logger: { info: () => undefined } },
        refreshDiagnostics: () => undefined,
      },
    } as unknown as ts.server.PluginCreateInfo);
    const resolved = host.resolveModuleNameLiterals(
      [{ text: './x.baml' } as ts.StringLiteralLike],
      '/p/index.ts',
    )[0]?.resolvedModule as ts.ResolvedModuleFull | undefined;
    expect(resolved?.extension).toBe(ts.Extension.Dts);
    expect(resolved?.resolvedFileName).toContain('/.baml/__virtual__/p_');
    expect(
      host.getScriptSnapshot(resolved?.resolvedFileName ?? '')?.getText(0, 100),
    ).toContain('declare const b: never');
  });

  it('keys virtual files by the importer-resolved source, not the raw specifier', () => {
    const plugin = init({ typescript: ts });
    const host = {
      fileExists: () => false,
      getScriptKind: () => ts.ScriptKind.TS,
      getScriptSnapshot: () => undefined,
      getScriptVersion: () => '1',
      readFile: () => undefined,
      resolveModuleNameLiterals: (
        literals: readonly ts.StringLiteralLike[],
        _containingFile?: string,
      ) => literals.map(() => ({ resolvedModule: undefined })),
    };
    const languageService = {
      getDefinitionAtPosition: () => undefined,
      getSemanticDiagnostics: () => [],
    };
    plugin.create({
      config: { root: '/p' },
      languageService,
      languageServiceHost: host,
      project: {
        getCurrentDirectory: () => '/p',
        projectService: { logger: { info: () => undefined } },
        refreshDiagnostics: () => undefined,
      },
    } as unknown as ts.server.PluginCreateInfo);
    // `./schema.baml` from two different directories names two different
    // physical sources; each must resolve to its own virtual declaration.
    const first = host.resolveModuleNameLiterals(
      [{ text: './schema.baml' } as ts.StringLiteralLike],
      '/p/a/one.ts',
    )[0]?.resolvedModule as ts.ResolvedModuleFull | undefined;
    const second = host.resolveModuleNameLiterals(
      [{ text: './schema.baml' } as ts.StringLiteralLike],
      '/p/b/two.ts',
    )[0]?.resolvedModule as ts.ResolvedModuleFull | undefined;
    expect(first?.resolvedFileName).toBeDefined();
    expect(second?.resolvedFileName).toBeDefined();
    expect(first?.resolvedFileName).not.toBe(second?.resolvedFileName);
    const third = host.resolveModuleNameLiterals(
      [{ text: './schema.baml' } as ts.StringLiteralLike],
      '/p/a/three.ts',
    )[0]?.resolvedModule as ts.ResolvedModuleFull | undefined;
    expect(third?.resolvedFileName).toBe(first?.resolvedFileName);
  });

  it('does not alias distinct sources that collide under the old 32-bit hash', () => {
    const plugin = init({ typescript: ts });
    const host = {
      fileExists: () => false,
      getScriptKind: () => ts.ScriptKind.TS,
      getScriptSnapshot: () => undefined,
      getScriptVersion: () => '1',
      readFile: () => undefined,
      resolveModuleNameLiterals: (
        literals: readonly ts.StringLiteralLike[],
        _containingFile?: string,
      ) => literals.map(() => ({ resolvedModule: undefined })),
    };
    plugin.create({
      config: { root: '/p' },
      languageService: {
        getDefinitionAtPosition: () => undefined,
        getSemanticDiagnostics: () => [],
      },
      languageServiceHost: host,
      project: {
        getCurrentDirectory: () => '/p',
        projectService: { logger: { info: () => undefined } },
        refreshDiagnostics: () => undefined,
      },
    } as unknown as ts.server.PluginCreateInfo);

    // `/p\0/p/1ilhgvv17hg/schema.baml` and
    // `/p\0/p/whhw8g1a1j/schema.baml` both hash to f45bed6c under the old
    // FNV-style key. Since their basenames also match, that implementation
    // returned one virtual file for two physical sources.
    const first = host.resolveModuleNameLiterals(
      [{ text: './schema.baml' } as ts.StringLiteralLike],
      '/p/1ilhgvv17hg/one.ts',
    )[0]?.resolvedModule as ts.ResolvedModuleFull | undefined;
    const second = host.resolveModuleNameLiterals(
      [{ text: './schema.baml' } as ts.StringLiteralLike],
      '/p/whhw8g1a1j/two.ts',
    )[0]?.resolvedModule as ts.ResolvedModuleFull | undefined;
    expect(first?.resolvedFileName).toBeDefined();
    expect(second?.resolvedFileName).toBeDefined();
    expect(first?.resolvedFileName).not.toBe(second?.resolvedFileName);
  });

  it('keys bare dependency specifiers by their Node-resolved physical path', async () => {
    // 'dep/baml_src/x.baml' imported from any directory names the same
    // physical source inside node_modules — one virtual declaration, not one
    // per importer, and never a made-up path under the importer's directory.
    const app = await realpath(await mkdtemp(join(tmpdir(), 'baml-plugin-')));
    const dependency = join(app, 'node_modules', 'dep');
    await mkdir(join(dependency, 'baml_src'), { recursive: true });
    await writeFile(
      join(dependency, 'package.json'),
      JSON.stringify({ name: 'dep', version: '1.0.0' }),
    );
    await writeFile(join(dependency, 'baml_src', 'x.baml'), 'class X {}\n');
    const plugin = init({ typescript: ts });
    const host = {
      fileExists: () => false,
      getScriptKind: () => ts.ScriptKind.TS,
      getScriptSnapshot: () => undefined,
      getScriptVersion: () => '1',
      readFile: () => undefined,
      resolveModuleNameLiterals: (
        literals: readonly ts.StringLiteralLike[],
        _containingFile?: string,
      ) => literals.map(() => ({ resolvedModule: undefined })),
    };
    plugin.create({
      config: { root: app },
      languageService: {
        getDefinitionAtPosition: () => undefined,
        getSemanticDiagnostics: () => [],
      },
      languageServiceHost: host,
      project: {
        getCurrentDirectory: () => app,
        projectService: { logger: { info: () => undefined } },
        refreshDiagnostics: () => undefined,
      },
    } as unknown as ts.server.PluginCreateInfo);
    const fromA = host.resolveModuleNameLiterals(
      [{ text: 'dep/baml_src/x.baml' } as ts.StringLiteralLike],
      join(app, 'a', 'one.ts'),
    )[0]?.resolvedModule as ts.ResolvedModuleFull | undefined;
    const fromB = host.resolveModuleNameLiterals(
      [{ text: 'dep/baml_src/x.baml' } as ts.StringLiteralLike],
      join(app, 'b', 'two.ts'),
    )[0]?.resolvedModule as ts.ResolvedModuleFull | undefined;
    expect(fromA?.resolvedFileName).toBeDefined();
    expect(fromA?.resolvedFileName).toBe(fromB?.resolvedFileName);
  });
});
