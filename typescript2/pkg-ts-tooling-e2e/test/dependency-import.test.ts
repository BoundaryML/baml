import { execFile } from 'node:child_process';
import {
  mkdir,
  mkdtemp,
  readdir,
  realpath,
  symlink,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { createBamlUnplugin } from '@boundaryml/baml-tooling/unplugin';
import { build as viteBuild } from 'vite';
import { describe, expect, it } from 'vitest';

const execFileAsync = promisify(execFile);
process.env.BAML_TOOLING_BRIDGE_PATH ??= join(
  import.meta.dirname,
  '../../../baml_language/sdks/typescript/bridge_tooling/baml-tooling-node.js',
);

async function bundle(entry: string, root: string): Promise<string> {
  const result = await viteBuild({
    build: {
      minify: false,
      rollupOptions: {
        external: [/^@boundaryml\//],
        input: entry,
      },
      write: false,
    },
    logLevel: 'silent',
    plugins: [
      createBamlUnplugin().vite({
        root,
      }) as unknown as import('vite').PluginOption,
    ],
    root,
  });
  const output = Array.isArray(result) ? result[0] : result;
  if (!output || !('output' in output))
    throw new Error('vite build produced no output');
  return output.output
    .map((chunk) => ('code' in chunk ? chunk.code : ''))
    .join('\n');
}

describe('dependency and monorepo .baml imports', () => {
  it('imports .baml from a packed dependency installed inside node_modules', async () => {
    const directory = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-dep-import-')),
    );
    const depSource = join(directory, 'dep-src');
    await mkdir(join(depSource, 'baml_src'), { recursive: true });
    await writeFile(
      join(depSource, 'package.json'),
      JSON.stringify({
        files: ['baml_src', 'baml.toml'],
        name: 'baml-dep',
        type: 'module',
        version: '1.0.0',
      }),
    );
    await writeFile(join(depSource, 'baml.toml'), '');
    await writeFile(
      join(depSource, 'baml_src', 'widget.baml'),
      'class Widget { label string }\nfunction Describe(widget: Widget) -> string { "dep" }\n',
    );
    await execFileAsync('npm', [
      'pack',
      depSource,
      '--pack-destination',
      directory,
    ]);

    const app = join(directory, 'app');
    await mkdir(app);
    await writeFile(
      join(app, 'package.json'),
      JSON.stringify({ private: true, type: 'module' }),
    );
    await writeFile(join(app, 'baml.toml'), '');
    await writeFile(
      join(app, 'main.baml'),
      'class Local { count int }\nfunction Read(local: Local) -> string { "app" }\n',
    );
    const tarball = (await readdir(directory)).find((name) =>
      name.endsWith('.tgz'),
    );
    if (!tarball) throw new Error('npm pack produced no tarball');
    await execFileAsync(
      'npm',
      [
        'install',
        '--ignore-scripts',
        '--no-package-lock',
        join(directory, tarball),
      ],
      { cwd: app },
    );
    const entry = join(app, 'entry.ts');
    await writeFile(
      entry,
      "import { b as local } from './main.baml';\nimport { b as dep, Widget } from 'baml-dep/baml_src/widget.baml';\ndeclare const widget: Widget;\nexport const result = [local.Read({ count: 1 }), dep.Describe(widget)] as const;\n",
    );

    // One build, two BAML projects: the app root and the dependency's own
    // baml.toml root inside node_modules. Both must emit through their own
    // sessions, runtime modules, and virtual modules — the entry's own
    // source mentions these names too, so assert on the emitted virtual
    // module shape, not the names alone.
    const code = await bundle(entry, app);
    expect(code.match(/initializeRuntimeFromBytecode\(bytecode/g)).toHaveLength(
      2,
    );
    expect(code).toMatch(/Read:\s*defineFunction\(/);
    expect(code).toMatch(/Describe:\s*defineFunction\(/);
  }, 120_000);

  it('imports .baml across pnpm-style monorepo workspace symlinks', async () => {
    const workspace = await realpath(
      await mkdtemp(join(tmpdir(), 'baml-mono-import-')),
    );
    const shared = join(workspace, 'packages', 'shared');
    const app = join(workspace, 'packages', 'app');
    await mkdir(join(shared, 'baml_src'), { recursive: true });
    await mkdir(join(app, 'node_modules', '@shared'), { recursive: true });
    await writeFile(
      join(shared, 'package.json'),
      JSON.stringify({
        name: '@shared/baml',
        type: 'module',
        version: '0.0.0',
      }),
    );
    await writeFile(join(shared, 'baml.toml'), '');
    await writeFile(
      join(shared, 'baml_src', 'shared.baml'),
      'class Shared { note string }\nfunction Summarize(shared: Shared) -> string { "shared" }\n',
    );
    await writeFile(join(app, 'baml.toml'), '');
    await writeFile(
      join(app, 'app.baml'),
      'class AppLocal { flag bool }\nfunction Check(app: AppLocal) -> string { "app" }\n',
    );
    // pnpm links workspace packages: node_modules/@shared/baml is a symlink
    // to the sibling package, so walk-up discovery must resolve the real
    // path before looking for the owning baml.toml.
    await symlink(shared, join(app, 'node_modules', '@shared', 'baml'), 'dir');
    const entry = join(app, 'entry.ts');
    await writeFile(
      entry,
      "import { b as app } from './app.baml';\nimport { b as shared, Shared } from '@shared/baml/baml_src/shared.baml';\ndeclare const value: Shared;\nexport const result = [app.Check({ flag: true }), shared.Summarize(value)] as const;\n",
    );

    const code = await bundle(entry, app);
    expect(code.match(/initializeRuntimeFromBytecode\(bytecode/g)).toHaveLength(
      2,
    );
    expect(code).toMatch(/Check:\s*defineFunction\(/);
    expect(code).toMatch(/Summarize:\s*defineFunction\(/);
  }, 120_000);
});
