import { mkdir, mkdtemp, realpath, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { BamlBuildCore } from '../src/build-core.js';
import { projectId } from '../src/project.js';
import { createBamlUnplugin } from '../src/unplugin.js';

const layout = {
  configPath: '/p/baml.toml',
  roots: ['/p'],
  sourceFiles: ['/p/x.baml'],
  watchFiles: ['/p/baml.toml', '/p/x.baml'],
};

const project = {
  check: () => ({ diagnostics: [], revision: 1 }),
  dispose: vi.fn(),
  layout: () => layout,
  refreshLayout: () => layout,
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

function callHook(
  plugin: unknown,
  name: string,
  context: unknown,
  ...args: unknown[]
): unknown {
  const hook = (plugin as Record<string, unknown>)[name] as
    | ((...args: unknown[]) => unknown)
    | { handler: (...args: unknown[]) => unknown }
    | undefined;
  if (!hook) throw new Error(`missing ${name} hook`);
  const handler = typeof hook === 'function' ? hook : hook.handler;
  return handler.apply(context, args);
}

describe('build core', () => {
  it('registers config from day one and shares one runtime module', async () => {
    const core = new BamlBuildCore(
      { root: '/p' },
      async () => project as never,
    );
    const watched: string[] = [];
    await core.start({ addWatchFile: (path) => watched.push(path) });
    expect(watched).toContain('/p/baml.toml');
    const id = await core.resolve('./x.baml', '/p/app.ts');
    expect(id).toMatch(/^\0baml:.*:file:/);
    if (!id) throw new Error('expected BAML module to resolve');
    expect(core.load(id)?.code).toContain('/p/x.baml');
    expect(await core.resolve('not-baml', '/p/app.ts')).toBeUndefined();
    const fullReload = vi.fn();
    core.watchChange('/p/baml.toml', '[package]\nname = "changed"\n', {
      fullReload,
    });
    expect(project.updateFile).toHaveBeenCalledWith(
      '/p/baml.toml',
      '[package]\nname = "changed"\n',
    );
    expect(fullReload).toHaveBeenCalledOnce();
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
    const id = await core.resolve('./x.baml', '/p/app.ts');
    if (!id) throw new Error('expected BAML module to resolve');
    expect(() => core.load(id)).toThrow('bad type');
  });

  it('ignores non-BAML watch changes instead of poisoning the project', async () => {
    const updateFile = vi.fn();
    const core = new BamlBuildCore(
      { root: '/p' },
      async () => ({ ...project, updateFile }) as never,
    );
    await core.start({ development: true });
    const invalidate = vi.fn();
    for (const changed of ['/p/src/App.tsx', '/p/styles.css', '/p/x.baml.ts'])
      core.watchChange(changed, 'edited', { invalidate });
    expect(updateFile).not.toHaveBeenCalled();
    expect(invalidate).not.toHaveBeenCalled();
    core.watchChange('/p/x.baml', 'class One {}\n', { invalidate });
    expect(updateFile).toHaveBeenCalledWith('/p/x.baml', 'class One {}\n');
  });

  it('ignores non-BAML edits through the Vite and Rollup adapter hooks', async () => {
    const viteUpdateFile = vi.fn();
    const vite = createBamlUnplugin(
      async () => ({ ...project, updateFile: viteUpdateFile }) as never,
    ).vite({ root: '/p' });
    await callHook(vite, 'buildStart', { addWatchFile: vi.fn() });
    callHook(vite, 'configureServer', {}, {});
    const invalidateModule = vi.fn();
    const fullReload = vi.fn();
    const viteResult = callHook(
      vite,
      'handleHotUpdate',
      { warn: vi.fn() },
      {
        file: '/p/src/App.tsx',
        modules: [{}],
        server: {
          moduleGraph: {
            getModuleById: vi.fn(),
            invalidateModule,
          },
          ws: { send: fullReload },
        },
      },
    );
    expect(viteResult).toBeUndefined();
    expect(viteUpdateFile).not.toHaveBeenCalled();
    expect(invalidateModule).not.toHaveBeenCalled();
    expect(fullReload).not.toHaveBeenCalled();

    const rollupUpdateFile = vi.fn();
    const rollup = createBamlUnplugin(
      async () => ({ ...project, updateFile: rollupUpdateFile }) as never,
    ).rollup({ root: '/p' });
    const addWatchFile = vi.fn();
    await callHook(rollup, 'buildStart', { addWatchFile });
    const invalidate = vi.fn();
    callHook(
      rollup,
      'watchChange',
      { invalidate, warn: vi.fn() },
      '/p/src/App.tsx',
      { event: 'update' },
    );
    expect(rollupUpdateFile).not.toHaveBeenCalled();
    expect(invalidate).not.toHaveBeenCalled();
  });

  it('matches only the exact project config path in watch events', async () => {
    const updateFile = vi.fn();
    const core = new BamlBuildCore(
      { root: '/p' },
      async () => ({ ...project, updateFile }) as never,
    );
    await core.start({ development: true });
    // A suffix match would forward these to the Rust session, which rejects
    // them with NonBamlPath — they must be filtered host-side.
    for (const changed of [
      '/p/mybaml.toml',
      '/p/not-baml.toml',
      '/p/sub/baml.toml',
    ])
      core.watchChange(changed, '[package]\nname = "nope"\n');
    expect(updateFile).not.toHaveBeenCalled();
    core.watchChange('/p/baml.toml', '[package]\nname = "yes"\n');
    expect(updateFile).toHaveBeenCalledWith(
      '/p/baml.toml',
      '[package]\nname = "yes"\n',
    );
  });

  it('never lets a rejected watch update throw out of the hook', async () => {
    const updateFile = vi.fn(() => {
      throw new Error('not a BAML source or baml.toml path: /p/x.baml');
    });
    const core = new BamlBuildCore(
      { root: '/p' },
      async () => ({ ...project, updateFile }) as never,
    );
    await core.start({ development: true });
    const watchError = vi.fn();
    expect(() =>
      core.watchChange('/p/x.baml', 'edited', { watchError }),
    ).not.toThrow();
    expect(watchError).toHaveBeenCalledOnce();
  });

  it('serves last good in development but fails production watch builds', async () => {
    const fixture = () => {
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
      return watchedProject;
    };
    // Development hosts (Vite dev) keep serving the last good module.
    const development = new BamlBuildCore(
      { root: '/p' },
      async () => fixture() as never,
    );
    await development.start({ development: true });
    const devId = await development.resolve('./x.baml', '/p/app.ts');
    if (!devId) throw new Error('expected BAML module to resolve');
    const good = development.load(devId);
    development.watchChange('/p/x.baml', 'invalid');
    expect(development.load(devId)).toEqual(good);
    // A watch event alone must not flip a production `rollup --watch`
    // build into serving stale output: the compiler error surfaces.
    const production = new BamlBuildCore(
      { root: '/p' },
      async () => fixture() as never,
    );
    await production.start();
    const prodId = await production.resolve('./x.baml', '/p/app.ts');
    if (!prodId) throw new Error('expected BAML module to resolve');
    production.load(prodId);
    production.watchChange('/p/x.baml', 'invalid');
    expect(() => production.load(prodId)).toThrow('broken edit');
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
    const id = await core.resolve('./x.baml', '/p/app.ts');
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

  it('routes dependency .baml imports to the dependency-owning project', async () => {
    const directory = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-core-dep-')),
    );
    const dep = join(directory, 'node_modules', 'baml-dep');
    await mkdir(join(dep, 'baml_src'), { recursive: true });
    await writeFile(join(directory, 'baml.toml'), '');
    await writeFile(join(directory, 'main.baml'), 'class Local {}\n');
    await writeFile(join(dep, 'baml.toml'), '');
    await writeFile(join(dep, 'baml_src', 'widget.baml'), 'class Widget {}\n');

    const opened: string[] = [];
    const factory = async (options: { cwd?: string }) => {
      const root = await realpath(options.cwd ?? directory);
      opened.push(root);
      const fakeLayout = {
        configPath: join(root, 'baml.toml'),
        roots: [root],
        sourceFiles:
          root === dep
            ? [join(dep, 'baml_src', 'widget.baml')]
            : [join(directory, 'main.baml')],
        watchFiles: [join(root, 'baml.toml')],
      };
      return {
        check: () => ({ diagnostics: [], revision: 1 }),
        dispose: vi.fn(),
        layout: () => fakeLayout,
        refreshLayout: () => fakeLayout,
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
          watchFiles: fakeLayout.watchFiles,
        }),
        resolveModule: (id: string) => ({
          code: `export const owner = ${JSON.stringify(root)}; export const id = ${JSON.stringify(id)}`,
          watchFiles: fakeLayout.watchFiles,
        }),
        updateFile: vi.fn(),
      } as never;
    };
    const core = new BamlBuildCore({ root: directory }, factory);
    const watched: string[] = [];
    await core.start({ addWatchFile: (path) => watched.push(path) });

    const appId = await core.resolve(
      './main.baml',
      join(directory, 'entry.ts'),
    );
    // Bundlers resolve concurrently: two racing imports from the same
    // dependency must open its project exactly once.
    await writeFile(join(dep, 'baml_src', 'gadget.baml'), 'class Gadget {}\n');
    const [depId, gadgetId] = await Promise.all([
      core.resolve(join(dep, 'baml_src', 'widget.baml'), join(dep, 'index.ts')),
      core.resolve(join(dep, 'baml_src', 'gadget.baml'), join(dep, 'other.ts')),
    ]);
    if (!appId || !depId || !gadgetId)
      throw new Error('expected all imports to resolve');
    expect(opened).toEqual([directory, dep]);
    // Virtual module ids are keyed by their owning project so two projects
    // coexist in one build without collisions.
    expect(appId).toContain(projectId(directory));
    expect(depId).toContain(projectId(dep));
    expect(depId).not.toContain(projectId(directory));
    expect(core.load(depId)?.code).toContain(dep);
    expect(watched).toContain(join(dep, 'baml.toml'));
    core.close();
  });

  it('routes symlinked workspace imports to the real sibling project', async () => {
    const workspaceDirectory = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-core-mono-')),
    );
    const shared = join(workspaceDirectory, 'packages', 'shared');
    const app = join(workspaceDirectory, 'packages', 'app');
    await mkdir(join(shared, 'baml_src'), { recursive: true });
    await mkdir(join(app, 'node_modules', '@shared'), { recursive: true });
    await writeFile(join(shared, 'package.json'), '{"name":"@shared/baml"}');
    await writeFile(join(shared, 'baml.toml'), '');
    await writeFile(join(shared, 'baml_src', 's.baml'), 'class Shared {}\n');
    await writeFile(join(app, 'baml.toml'), '');
    await symlink(shared, join(app, 'node_modules', '@shared', 'baml'), 'dir');

    const opened: string[] = [];
    const factory = async (options: { cwd?: string }) => {
      const root = await realpath(options.cwd ?? app);
      opened.push(root);
      const fakeLayout = {
        configPath: join(root, 'baml.toml'),
        roots: [root],
        sourceFiles:
          root === shared ? [join(shared, 'baml_src', 's.baml')] : [],
        watchFiles: [join(root, 'baml.toml')],
      };
      return {
        check: () => ({ diagnostics: [], revision: 1 }),
        dispose: vi.fn(),
        layout: () => fakeLayout,
        refreshLayout: () => fakeLayout,
        resolveDts: () => ({
          code: 'export declare const id: string',
          map: {
            generatedFile: 's.baml',
            segments: [],
            sourceHashes: [],
            sources: [],
            version: 1,
          },
          stale: false,
          watchFiles: fakeLayout.watchFiles,
        }),
        resolveModule: () => ({ code: 'export {}', watchFiles: [] }),
        updateFile: vi.fn(),
      } as never;
    };
    const core = new BamlBuildCore({ root: app }, factory);
    await core.start();
    // A bare workspace specifier resolves through the pnpm-style symlink to
    // the real sibling package, whose own baml.toml owns the source.
    const id = await core.resolve(
      '@shared/baml/baml_src/s.baml',
      join(app, 'entry.ts'),
    );
    if (!id) throw new Error('expected workspace import to resolve');
    expect(opened).toEqual([app, shared]);
    expect(id).toContain(projectId(shared));
    core.close();
  });

  it('keeps bundling after baml-ts-gen wrote declaration sidecars', async () => {
    // `baml-ts-gen` writes `<source>.baml.d.ts` beside every source so plain
    // `tsc --noEmit` works. Deferring to the bundler because that file exists
    // would hand vite/rollup/esbuild the raw `.baml` path, which they resolve
    // literally and fail to parse as JavaScript — the two documented
    // workflows must coexist in one checkout.
    const directory = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-core-sidecar-')),
    );
    await writeFile(join(directory, 'baml.toml'), '');
    await writeFile(join(directory, 'main.baml'), 'enum Color { Red }\n');
    await writeFile(
      join(directory, 'main.baml.d.ts'),
      '// baml-fingerprint: abc\nexport declare const b: unknown;\n',
    );
    const fakeLayout = {
      configPath: join(directory, 'baml.toml'),
      roots: [directory],
      sourceFiles: [join(directory, 'main.baml')],
      watchFiles: [join(directory, 'baml.toml')],
    };
    const core = new BamlBuildCore(
      { root: directory },
      async () =>
        ({
          ...project,
          layout: () => fakeLayout,
          refreshLayout: () => fakeLayout,
        }) as never,
    );
    await core.start();
    const id = await core.resolve('./main.baml', join(directory, 'app.ts'));
    expect(id).toMatch(/^\0baml:.*:file:/);
    if (!id)
      throw new Error('a declaration sidecar must not disable the plugin');
    expect(core.load(id)?.code).toContain(join(directory, 'main.baml'));

    // The full `<source>.baml.ts` sidecar does carry runtime code, so it wins
    // — but it is resolved explicitly. Returning undefined would leave the
    // bundler resolving `./main.baml` to the `.baml` file, never the sidecar.
    await writeFile(
      join(directory, 'main.baml.ts'),
      '// baml-fingerprint: abc\nexport const b = {};\n',
    );
    expect(await core.resolve('./main.baml', join(directory, 'app.ts'))).toBe(
      join(directory, 'main.baml.ts'),
    );
    core.close();
  });

  it('registers .baml files created after the project opened', async () => {
    const directory = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-core-new-')),
    );
    await writeFile(join(directory, 'baml.toml'), '');
    await writeFile(join(directory, 'main.baml'), 'class One {}\n');
    let sources = [join(directory, 'main.baml')];
    const updateFile = vi.fn((_path: string, _text: string) => {
      sources = [...sources, join(directory, 'added.baml')];
    });
    const core = new BamlBuildCore({ root: directory }, async () => {
      const fakeLayout = () => ({
        configPath: join(directory, 'baml.toml'),
        roots: [directory],
        sourceFiles: sources,
        watchFiles: [join(directory, 'baml.toml'), ...sources],
      });
      return {
        check: () => ({ diagnostics: [], revision: 1 }),
        dispose: vi.fn(),
        layout: fakeLayout,
        refreshLayout: fakeLayout,
        resolveDts: () => ({
          code: 'export declare const id: string',
          map: {
            generatedFile: 'added.baml',
            segments: [],
            sourceHashes: [],
            sources: [],
            version: 1,
          },
          stale: false,
          watchFiles: [],
        }),
        resolveModule: () => ({ code: 'export {}', watchFiles: [] }),
        updateFile,
      } as never;
    });
    const watched: string[] = [];
    await core.start({ addWatchFile: (path) => watched.push(path) });
    expect(watched).not.toContain(join(directory, 'added.baml'));
    await writeFile(join(directory, 'added.baml'), 'class Two {}\n');
    const id = await core.resolve('./added.baml', join(directory, 'entry.ts'));
    if (!id) throw new Error('expected the new import to resolve');
    expect(updateFile).toHaveBeenCalledWith(
      join(directory, 'added.baml'),
      'class Two {}\n',
    );
    expect(watched).toContain(join(directory, 'added.baml'));
    core.close();
  });
});
