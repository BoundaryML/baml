import { createRequire } from 'node:module';
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
});
