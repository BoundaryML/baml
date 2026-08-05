import { describe, expect, it, vi } from 'vitest';
import { BamlBuildCore } from '../src/build-core.js';

const project = {
  check: () => ({ diagnostics: [], revision: 1 }),
  dispose: vi.fn(),
  layout: () => ({
    configPath: '/p/baml.toml',
    roots: ['/p'],
    sourceFiles: ['/p/x.baml'],
    watchFiles: ['/p/baml.toml', '/p/x.baml'],
  }),
  resolveDts: (id: string) => ({
    code: `export declare const id: ${JSON.stringify(id)}`,
    map: {
      generatedFile: id,
      segments: [],
      sourceHashes: [],
      sources: [],
      version: 1,
    },
    stale: false,
    watchFiles: ['/p/x.baml'],
  }),
  resolveModule: (id: string) => ({
    code: `export const id = ${JSON.stringify(id)}`,
    watchFiles: ['/p/x.baml'],
  }),
  updateFile: vi.fn(),
};

describe('build core', () => {
  it('registers config from day one and shares one runtime module', async () => {
    const core = new BamlBuildCore(
      { root: '/p' },
      async () => project as never,
    );
    const watched: string[] = [];
    await core.start({ addWatchFile: (path) => watched.push(path) });
    expect(watched).toContain('/p/baml.toml');
    const id = core.resolve('./x.baml', '/p/app.ts');
    expect(id).toMatch(/^\\0baml:.*:file:/);
    if (!id) throw new Error('expected BAML module to resolve');
    expect(core.load(id)?.code).toContain('/p/x.baml');
    expect(core.resolve('not-baml', '/p/app.ts')).toBeUndefined();
  });

  it('maps compiler errors to the physical BAML location', async () => {
    const broken = {
      ...project,
      check: () => ({
        diagnostics: [
          {
            code: 'x',
            location: { lengthUtf8: 1, path: '/p/x.baml', startUtf8: 4 },
            message: 'bad type',
            severity: 'error',
          },
        ],
        revision: 2,
      }),
    };
    const core = new BamlBuildCore({ root: '/p' }, async () => broken as never);
    await core.start();
    const id = core.resolve('./x.baml', '/p/app.ts');
    if (!id) throw new Error('expected BAML module to resolve');
    expect(() => core.load(id)).toThrow('bad type');
  });

  it('keeps the last good module during a failed watch revision', async () => {
    let broken = false;
    let implementation = 'one';
    const watchedProject = {
      ...project,
      check: () => ({
        diagnostics: broken
          ? [{ message: 'broken edit', severity: 'error' }]
          : [],
        revision: broken ? 2 : 1,
      }),
      resolveModule: () => ({
        code: `export const implementation = ${JSON.stringify(implementation)}`,
        watchFiles: ['/p/x.baml'],
      }),
      updateFile: () => {
        broken = true;
        implementation = 'invalid';
      },
    };
    const core = new BamlBuildCore(
      { root: '/p' },
      async () => watchedProject as never,
    );
    await core.start();
    const id = core.resolve('./x.baml', '/p/app.ts');
    if (!id) throw new Error('expected BAML module to resolve');
    const good = core.load(id);
    core.watchChange('/p/x.baml', 'invalid');
    expect(core.load(id)).toEqual(good);
  });

  it('invalidates changed runtime output and reloads only for export-shape changes', async () => {
    let declaration = 'export declare const value: string';
    let implementation = 'one';
    const watchedProject = {
      ...project,
      resolveDts: () => ({
        code: declaration,
        map: {
          generatedFile: 'x.baml',
          segments: [],
          sourceHashes: [],
          sources: [],
          version: 1,
        },
        stale: false,
        watchFiles: ['/p/x.baml'],
      }),
      resolveModule: () => ({
        code: `export const value = ${JSON.stringify(implementation)}`,
        watchFiles: ['/p/x.baml'],
      }),
      updateFile: (_path: string, text: string) => {
        implementation = text;
        if (text === 'shape')
          declaration = 'export declare const value: number';
      },
    };
    const core = new BamlBuildCore(
      { root: '/p' },
      async () => watchedProject as never,
    );
    await core.start();
    const id = core.resolve('./x.baml', '/p/app.ts');
    if (!id) throw new Error('expected BAML module to resolve');
    core.load(id);
    const invalidate = vi.fn();
    const fullReload = vi.fn();
    core.watchChange('/p/x.baml', 'two', { fullReload, invalidate });
    expect(invalidate).toHaveBeenCalledWith(id);
    expect(invalidate).toHaveBeenCalledWith(expect.stringMatching(/:runtime$/));
    expect(fullReload).not.toHaveBeenCalled();
    core.watchChange('/p/x.baml', 'shape', { fullReload, invalidate });
    expect(fullReload).toHaveBeenCalledOnce();
  });
});
