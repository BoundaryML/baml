import { execFile } from 'node:child_process';
import {
  copyFile,
  mkdir,
  mkdtemp,
  readdir,
  realpath,
  writeFile,
} from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, resolve } from 'node:path';
import { promisify } from 'node:util';
import { describe, expect, it } from 'vitest';

const execFileAsync = promisify(execFile);
const workspace = resolve(import.meta.dirname, '../..');
const repository = resolve(workspace, '..');
const bridge = resolve(
  repository,
  'baml_language/sdks/typescript/bridge_tooling',
);
const runtimeBridge = resolve(
  repository,
  'baml_language/sdks/typescript/bridge_typescript',
);

function cleanEnvironment(): NodeJS.ProcessEnv {
  const environment = { ...process.env };
  delete environment.BAML_TOOLING_BRIDGE_PATH;
  delete environment.INIT_CWD;
  return environment;
}

describe('packed clean consumer', () => {
  it('drives compiler, navigation, declarations, sidecar tsc, and an executed runtime with zero internal env', async () => {
    const environment = cleanEnvironment();
    await execFileAsync(
      'pnpm',
      ['--filter', '@boundaryml/baml-tooling', 'build'],
      {
        cwd: workspace,
        env: environment,
      },
    );
    await execFileAsync('pnpm', ['build:debug'], {
      cwd: bridge,
      env: environment,
    });

    const directory = await realpath(
      await mkdtemp(resolve(tmpdir(), 'baml-packed-')),
    );
    await execFileAsync(
      'pnpm',
      [
        '--dir',
        resolve(workspace, 'pkg-baml-tooling'),
        'pack',
        '--pack-destination',
        directory,
      ],
      { cwd: workspace, env: environment },
    );
    await execFileAsync(
      'npm',
      ['pack', bridge, '--pack-destination', directory],
      { cwd: workspace, env: environment },
    );
    await execFileAsync(
      'npm',
      ['pack', runtimeBridge, '--pack-destination', directory],
      { cwd: workspace, env: environment },
    );

    const platform = `${process.platform}-${process.arch}`;
    const target =
      process.platform === 'linux'
        ? `${platform}-gnu`
        : process.platform === 'win32'
          ? `${platform}-msvc`
          : platform;
    const binary = `baml_tooling_node.${target}.node`;
    const platformDirectory = resolve(directory, `bridge-${target}`);
    await mkdir(platformDirectory);
    await copyFile(resolve(bridge, binary), resolve(platformDirectory, binary));
    const platformPackage = `@boundaryml/baml-bridge-tooling-${target}`;
    await writeFile(
      resolve(platformDirectory, 'package.json'),
      JSON.stringify({
        cpu: [process.arch],
        files: [binary],
        main: binary,
        name: platformPackage,
        os: [process.platform],
        version: '0.15.0',
      }),
    );
    await execFileAsync(
      'npm',
      ['pack', platformDirectory, '--pack-destination', directory],
      { cwd: workspace, env: environment },
    );

    const runtimeBinary = `baml_node.${target}.node`;
    const runtimePlatformDirectory = resolve(
      directory,
      `runtime-bridge-${target}`,
    );
    await mkdir(runtimePlatformDirectory);
    await copyFile(
      resolve(runtimeBridge, runtimeBinary),
      resolve(runtimePlatformDirectory, runtimeBinary),
    );
    await writeFile(
      resolve(runtimePlatformDirectory, 'package.json'),
      JSON.stringify({
        cpu: [process.arch],
        files: [runtimeBinary],
        main: runtimeBinary,
        name: `@boundaryml/baml-bridge-${target}`,
        os: [process.platform],
        version: '0.15.0',
      }),
    );
    await execFileAsync(
      'npm',
      ['pack', runtimePlatformDirectory, '--pack-destination', directory],
      { cwd: workspace, env: environment },
    );

    const archives = (await readdir(directory))
      .filter((name) => name.endsWith('.tgz'))
      .map((name) => resolve(directory, name));
    expect(archives).toHaveLength(5);
    await writeFile(
      resolve(directory, 'package.json'),
      JSON.stringify({ private: true, type: 'module' }),
    );
    await execFileAsync(
      'npm',
      [
        'install',
        '--ignore-scripts',
        '--no-package-lock',
        ...archives,
        'typescript@5.9.3',
        'esbuild@0.25.0',
        'vite@6.0.0',
      ],
      { cwd: directory, env: environment },
    );

    const baml = resolve(directory, 'main.baml');
    const source = `
type Json = null | bool | int | float | string | Json[] | map<string, Json>
type Choice = "a" | "b"
class Bag {
  scores map<string, int>
  maybe string?
  choice Choice
  picture image
  nested Bag[]
}
enum Color { Red Blue }
function Read(input: Bag) -> Json { null }
`;
    await writeFile(resolve(directory, 'baml.toml'), '');
    await writeFile(baml, source);
    await writeFile(
      resolve(directory, 'consumer.ts'),
      "import { b, Bag, Bag$stream, Color } from './main.baml';\nconst scores: Bag['scores'] = { a: 1 };\ndeclare const bag: Bag;\ntype Stream = Bag$stream;\nexport const result = [b.Read(bag), Color.Red, scores] as const;\n",
    );
    await writeFile(
      resolve(directory, 'runtime-consumer.ts'),
      `import { b, Color } from './main.baml';
if (typeof b.Read !== 'function') throw new Error('generated b.Read is not callable');
if (Color.Red !== 'Red') throw new Error('runtime enum export is missing');
`,
    );
    await writeFile(
      resolve(directory, 'tsconfig.json'),
      JSON.stringify({
        compilerOptions: {
          allowArbitraryExtensions: true,
          module: 'esnext',
          moduleResolution: 'bundler',
          noEmit: true,
          strict: true,
          target: 'es2022',
        },
        include: ['consumer.ts'],
      }),
    );

    const probe = resolve(directory, 'probe.mjs');
    await writeFile(
      probe,
      `import { loadProject } from '@boundaryml/baml-tooling';
const sourcePath = ${JSON.stringify(baml)};
const source = ${JSON.stringify(source)};
const project = await loadProject({ backend: 'native', cwd: process.cwd() });
if (project.check().diagnostics.length) throw new Error('unexpected diagnostics');
const declaration = project.resolveDts(sourcePath, sourcePath);
for (const expected of ['{ [key: string]: number }', 'string | null', 'baml.media.Image', 'Bag$stream']) if (!declaration.code.includes(expected)) throw new Error('missing declaration: ' + expected);
const offset = Buffer.byteLength(source.slice(0, source.indexOf('Bag {')), 'utf8');
const segment = declaration.map.segments.find((item) => item.symbolId === 'T:user.Bag');
if (!segment) throw new Error('missing Bag segment');
if (!project.definitionAt(sourcePath, offset)[0]?.path.endsWith('main.baml')) throw new Error('definition did not map to BAML');
if (project.referencesAt(sourcePath, offset).length < 2) throw new Error('references missing');
if (!project.hoverAt('', 0, segment.symbolId).markdown.includes('class Bag')) throw new Error('hover missing');
if (!project.completionsAt().some((item) => item.label === 'Bag')) throw new Error('completion missing');
if (!project.prepareRename(segment.symbolId).path.endsWith('main.baml')) throw new Error('prepare rename missing');
if (project.rename(segment.symbolId, 'Container').edits.length < 2) throw new Error('rename edits missing');
project.dispose();
`,
    );
    await execFileAsync(process.execPath, [probe], {
      cwd: directory,
      env: environment,
    });

    await execFileAsync(
      process.execPath,
      [
        resolve(
          directory,
          'node_modules/@boundaryml/baml-tooling/dist/bin/baml-ts-gen.js',
        ),
      ],
      { cwd: directory, env: environment },
    );
    await execFileAsync(
      process.execPath,
      [
        resolve(directory, 'node_modules/typescript/bin/tsc'),
        '-p',
        'tsconfig.json',
      ],
      { cwd: directory, env: environment },
    );

    const build = resolve(directory, 'build.mjs');
    await writeFile(
      build,
      `import { build, context } from 'esbuild';
import { access, readFile, writeFile } from 'node:fs/promises';
import { createBamlUnplugin } from '@boundaryml/baml-tooling/esbuild';
// The sidecar baml-ts-gen just wrote for the tsc run above stays on disk:
// declaration sidecars must not disable bundler virtualization, or the two
// documented workflows could not coexist in one checkout.
await access(${JSON.stringify(`${baml}.d.ts`)});
await build({ bundle: true, entryPoints: ['runtime-consumer.ts'], format: 'esm', outfile: 'runtime-output.mjs', packages: 'external', plugins: [createBamlUnplugin().esbuild({ root: process.cwd() })] });
const outfile = 'watched-output.mjs';
const ctx = await context({ bundle: true, entryPoints: ['consumer.ts'], format: 'esm', outfile, packages: 'external', plugins: [createBamlUnplugin().esbuild({ root: process.cwd() })] });
const waitFor = async (text) => {
  for (let attempt = 0; attempt < 100; attempt++) {
    try { if ((await readFile(outfile, 'utf8')).includes(text)) return; } catch {}
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error('timed out waiting for esbuild watch output: ' + text);
};
try {
  await ctx.watch();
  await waitFor('Red');
  await writeFile(${JSON.stringify(baml)}, ${JSON.stringify(source.replace('Red Blue', 'Red Blue Green'))});
  await waitFor('Green');
} finally {
  await ctx.dispose();
}
`,
    );
    await execFileAsync(process.execPath, [build], {
      cwd: directory,
      env: environment,
    });
    await execFileAsync(process.execPath, ['runtime-output.mjs'], {
      cwd: directory,
      env: environment,
    });

    const hmr = resolve(directory, 'hmr.mjs');
    await writeFile(
      hmr,
      `import { writeFile } from 'node:fs/promises';
import { createServer } from 'vite';
import { createBamlUnplugin } from '@boundaryml/baml-tooling/vite';
const bamlPath = ${JSON.stringify(baml)};
const original = ${JSON.stringify(source)};
await writeFile(bamlPath, original);
const server = await createServer({ appType: 'custom', logLevel: 'silent', plugins: [createBamlUnplugin().vite({ root: process.cwd() })], root: process.cwd(), server: { middlewareMode: true } });
const messages = [];
const send = server.ws.send.bind(server.ws);
server.ws.send = (payload, ...rest) => { messages.push(payload); return send(payload, ...rest); };
const waitForMessage = async (predicate, description) => {
  for (let attempt = 0; attempt < 100; attempt++) {
    if (messages.some(predicate)) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error('timed out waiting for Vite HMR ' + description + ': ' + JSON.stringify(messages));
};
try {
  const first = await server.transformRequest(bamlPath);
  if (!first?.code.includes('Color')) throw new Error('initial Vite transform missing BAML exports');
  messages.length = 0;
  await writeFile(bamlPath, original.replace('{ null }', '{ 1 }'));
  await waitForMessage((message) => message?.type === 'update' && message.updates?.some((update) => update.type === 'js-update'), 'module update');
  if (messages.some((message) => message?.type === 'full-reload')) throw new Error('implementation-only edit forced a full reload: ' + JSON.stringify(messages));
  const implementation = await server.transformRequest(bamlPath);
  if (!implementation?.code.includes('Color')) throw new Error('implementation HMR lost the module');
  messages.length = 0;
  const shaped = original.replace('nested Bag[]', 'nested Bag[]\\n  label string');
  await writeFile(bamlPath, shaped);
  await waitForMessage((message) => message?.type === 'full-reload', 'full reload');
  messages.length = 0;
  await writeFile(bamlPath, 'class Bag {');
  await waitForMessage((message) => message?.type === 'update' || message?.type === 'full-reload' || message?.type === 'error', 'compiler-error invalidation');
  const stale = await server.transformRequest(bamlPath);
  if (!stale?.code.includes('Color')) throw new Error('compiler error did not preserve last-good output');
  messages.length = 0;
  await writeFile(bamlPath, original);
  await waitForMessage((message) => message?.type === 'update' || message?.type === 'full-reload', 'recovery update');
  const recovered = await server.transformRequest(bamlPath);
  if (!recovered?.code.includes('Color')) throw new Error('Vite did not recover after fixing BAML');
} finally {
  await server.close();
}
`,
    );
    await execFileAsync(process.execPath, [hmr], {
      cwd: directory,
      env: environment,
    });

    const { stdout } = await execFileAsync(
      process.execPath,
      [
        '-e',
        "const plugin = require('@boundaryml/baml-tooling/typescript-plugin'); process.stdout.write(typeof plugin);",
      ],
      { cwd: directory, env: environment },
    );
    expect(stdout).toBe('function');
    expect(
      basename(archives.find((path) => path.includes(target)) ?? ''),
    ).toContain(target);
  }, 600_000);
});
