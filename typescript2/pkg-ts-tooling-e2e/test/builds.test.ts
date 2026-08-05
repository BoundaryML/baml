import { execFile } from 'node:child_process';
import { mkdtemp, readdir, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { promisify } from 'node:util';
import { projectId } from '@boundaryml/baml-tooling';
import { createBamlUnplugin } from '@boundaryml/baml-tooling/unplugin';
import { build as esbuild } from 'esbuild';
import { rollup } from 'rollup';
import { build as viteBuild } from 'vite';
import { describe, expect, it } from 'vitest';
import webpack from 'webpack';

const execFileAsync = promisify(execFile);
const nativeBridge = process.env.BAML_TOOLING_BRIDGE_PATH;

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), 'baml-builds-'));
  await writeFile(join(root, 'baml.toml'), '');
  await writeFile(join(root, 'person.baml'), 'class Person {}\n');
  const entry = join(root, 'entry.ts');
  await writeFile(
    entry,
    "import { b } from './person.baml'; export const result = b.Greet();\n",
  );
  return { entry, root };
}

function projectFactory(root: string) {
  const runtimeId = `\\0baml:${projectId(root)}:runtime`;
  return async () =>
    ({
      check: () => ({ diagnostics: [], revision: 1 }),
      dispose: () => undefined,
      layout: () => ({
        configPath: join(root, 'baml.toml'),
        roots: [root],
        sourceFiles: [join(root, 'person.baml')],
        watchFiles: [join(root, 'baml.toml'), join(root, 'person.baml')],
      }),
      resolveDts: () => ({
        code: 'export declare const b: { Greet(): string };',
        map: {
          generatedFile: 'person.baml.d.ts',
          segments: [],
          sourceHashes: [],
          sources: [],
          version: 1,
        },
        stale: false,
        watchFiles: [join(root, 'person.baml')],
      }),
      resolveModule: (id: string) => ({
        code:
          id === runtimeId
            ? "export const marker = 'one-program';"
            : `import { marker } from ${JSON.stringify(runtimeId)}; export const b = { Greet: () => marker };`,
        watchFiles: [join(root, 'person.baml')],
      }),
      updateFile: () => undefined,
    }) as never;
}

describe('real build hosts', () => {
  it('bundles direct BAML imports with Vite and one shared runtime', async () => {
    const { entry, root } = await fixture();
    const plugin = createBamlUnplugin(projectFactory(root));
    await viteBuild({
      build: {
        emptyOutDir: true,
        lib: { entry, formats: ['es'] },
        outDir: join(root, 'dist-vite'),
      },
      logLevel: 'silent',
      plugins: [
        plugin.vite({ root }) as unknown as import('vite').PluginOption,
      ],
      root,
    });
    const outputFile = (await readdir(join(root, 'dist-vite'))).find(
      (name) => name.endsWith('.js') || name.endsWith('.mjs'),
    );
    if (!outputFile) throw new Error('Vite did not emit a JavaScript bundle');
    const output = await readFile(join(root, 'dist-vite', outputFile), 'utf8');
    expect(output.match(/one-program/g)).toHaveLength(1);
  });

  it('bundles with Rollup and esbuild through the same core', async () => {
    const { entry, root } = await fixture();
    const plugin = createBamlUnplugin(projectFactory(root));
    const bundle = await rollup({
      input: entry,
      plugins: [plugin.rollup({ root })],
    });
    const rollupOutput = await bundle.generate({ format: 'es' });
    await bundle.close();
    expect(rollupOutput.output[0]?.code).toContain('one-program');

    const esbuildOutput = await esbuild({
      bundle: true,
      entryPoints: [entry],
      format: 'esm',
      plugins: [plugin.esbuild({ root })],
      write: false,
    });
    expect(esbuildOutput.outputFiles[0]?.text).toContain('one-program');
  });

  it('bundles with webpack', async () => {
    const { entry, root } = await fixture();
    const plugin = createBamlUnplugin(projectFactory(root));
    const outputPath = join(root, 'dist-webpack');
    await new Promise<void>((resolveBuild, rejectBuild) => {
      webpack(
        {
          entry,
          mode: 'development',
          output: { filename: 'bundle.js', path: outputPath },
          plugins: [plugin.webpack({ root })],
        },
        (error, stats) => {
          if (error || stats?.hasErrors())
            rejectBuild(error ?? new Error(stats?.toString()));
          else resolveBuild();
        },
      );
    });
    expect(await readFile(join(outputPath, 'bundle.js'), 'utf8')).toContain(
      'one-program',
    );
  });

  it('bundles with the direct Bun adapter', async () => {
    const { entry, root } = await fixture();
    const adapter = new URL(
      '../../pkg-baml-tooling/src/bun.ts',
      import.meta.url,
    ).pathname;
    const script = join(root, 'bun-build.ts');
    const runtimeId = `\\0baml:${projectId(root)}:runtime`;
    await writeFile(
      script,
      `import { setup } from ${JSON.stringify(adapter)};
const root = ${JSON.stringify(root)};
const runtimeId = ${JSON.stringify(runtimeId)};
const factory = async () => ({
  check: () => ({ diagnostics: [], revision: 1 }),
  dispose: () => {},
  layout: () => ({ configPath: root + '/baml.toml', roots: [root], sourceFiles: [root + '/person.baml'], watchFiles: [root + '/baml.toml', root + '/person.baml'] }),
  resolveDts: () => ({ code: 'export declare const b: { Greet(): string };', map: { generatedFile: 'x', segments: [], sourceHashes: [], sources: [], version: 1 }, stale: false, watchFiles: [] }),
  resolveModule: (id: string) => ({ code: id === runtimeId ? "export const marker = 'one-program';" : \`import { marker } from \${JSON.stringify(runtimeId)}; export const b = { Greet: () => marker };\`, watchFiles: [] }),
  updateFile: () => {},
});
const result = await Bun.build({ entrypoints: [${JSON.stringify(entry)}], plugins: [{ name: 'baml-test', setup(build) { setup(build, { root }, factory); } }] });
if (!result.success) throw new Error(result.logs.join('\\n'));
const text = await result.outputs[0].text();
console.log((text.match(/one-program/g) ?? []).length);
`,
    );
    const { stdout } = await execFileAsync('bun', [script]);
    expect(stdout.trim()).toBe('1');
  });
});

describe.runIf(nativeBridge)('compiler-backed builds', () => {
  it('bundles runtime enum values exported by a BAML module', async () => {
    const root = await mkdtemp(join(tmpdir(), 'baml-enum-build-'));
    await writeFile(join(root, 'baml.toml'), '');
    await writeFile(
      join(root, 'colors.baml'),
      'enum Color {\n  Red\n  Blue\n}\n',
    );
    const entry = join(root, 'entry.ts');
    await writeFile(
      entry,
      "import { Color } from './colors.baml'; export const result = Color.Red;\n",
    );

    const output = await esbuild({
      bundle: true,
      entryPoints: [entry],
      format: 'esm',
      packages: 'external',
      plugins: [createBamlUnplugin().esbuild({ root })],
      write: false,
    });
    expect(output.outputFiles[0]?.text).toContain('Red');
  });
});
