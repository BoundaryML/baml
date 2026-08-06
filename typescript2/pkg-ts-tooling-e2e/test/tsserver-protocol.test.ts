import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import {
  copyFile,
  mkdir,
  readdir,
  realpath,
  symlink,
  writeFile,
} from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const workspace = resolve(import.meta.dirname, '../..');
const repository = resolve(workspace, '..');
const bridge = resolve(
  repository,
  'baml_language/sdks/typescript/bridge_tooling',
);

function cleanEnvironment(): NodeJS.ProcessEnv {
  const environment = { ...process.env };
  delete environment.BAML_TOOLING_BRIDGE_PATH;
  delete environment.INIT_CWD;
  return environment;
}

async function installPackedToolchain(
  directory: string,
  environment: NodeJS.ProcessEnv,
): Promise<void> {
  await run(
    'pnpm',
    ['--filter', '@boundaryml/baml-tooling', 'build'],
    workspace,
    environment,
  );
  await run('pnpm', ['build:debug'], bridge, environment);
  await run(
    'pnpm',
    [
      '--dir',
      resolve(workspace, 'pkg-baml-tooling'),
      'pack',
      '--pack-destination',
      directory,
    ],
    workspace,
    environment,
  );
  await run(
    'npm',
    ['pack', bridge, '--pack-destination', directory],
    workspace,
    environment,
  );
  const platform = `${process.platform}-${process.arch}`;
  const target = process.platform === 'linux' ? `${platform}-gnu` : platform;
  const binary = `baml_tooling_node.${target}.node`;
  const platformDirectory = join(directory, `bridge-${target}`);
  await mkdir(platformDirectory);
  await copyFile(join(bridge, binary), join(platformDirectory, binary));
  await writeFile(
    join(platformDirectory, 'package.json'),
    JSON.stringify({
      cpu: [process.arch],
      files: [binary],
      main: binary,
      name: `@boundaryml/baml-bridge-tooling-${target}`,
      os: [process.platform],
      version: '0.15.0',
    }),
  );
  await run(
    'npm',
    ['pack', platformDirectory, '--pack-destination', directory],
    workspace,
    environment,
  );
  const archives = (await readdir(directory))
    .filter((name) => name.endsWith('.tgz'))
    .map((name) => join(directory, name));
  await writeFile(
    join(directory, 'package.json'),
    JSON.stringify({ private: true }),
  );
  await run(
    'npm',
    [
      'install',
      '--ignore-scripts',
      '--no-package-lock',
      ...archives,
      'typescript@5.9.3',
    ],
    directory,
    environment,
  );
}

function writeTsconfig(directory: string): Promise<void> {
  return writeFile(
    join(directory, 'tsconfig.json'),
    JSON.stringify({
      compilerOptions: {
        allowNonTsExtensions: true,
        module: 'esnext',
        moduleResolution: 'bundler',
        plugins: [
          {
            name: '@boundaryml/baml-tooling/typescript-plugin',
            root: directory,
          },
        ],
        strict: true,
        target: 'es2022',
      },
      files: ['consumer.ts'],
    }),
  );
}

describe('packed tsserver protocol', () => {
  it('returns physical BAML locations for editor protocol requests', async () => {
    const directory = join(
      await realpath(tmpdir()),
      `baml-tsserver-${process.pid}-${Date.now()}`,
    );
    await mkdir(directory);
    const environment = cleanEnvironment();
    await installPackedToolchain(directory, environment);
    const baml = join(directory, 'main.baml');
    const consumer = join(directory, 'consumer.ts');
    await writeFile(join(directory, 'baml.toml'), '');
    await writeFile(
      baml,
      '/// Person docs\nclass Person { name string }\n/// Greet docs\nfunction Greet(p: Person) -> string { "hi" }\n',
    );
    const source =
      "import { b, Person } from './main.baml';\nconst p: Person = { name: 'Ada' };\nexport const greeting = b.Greet(p);\n";
    await writeFile(consumer, source);
    await writeTsconfig(directory);

    const installedRequire = createRequire(join(directory, 'package.json'));
    const server = new Tsserver(
      installedRequire.resolve('typescript/lib/tsserver.js'),
      directory,
      environment,
    );
    try {
      await server.command('open', { file: consumer, fileContent: source });
      const projectInfo = await server.command('projectInfo', {
        file: consumer,
        needFileNameList: true,
      });
      expect(JSON.stringify(projectInfo)).toContain('tsconfig.json');
      await server.command('semanticDiagnosticsSync', { file: consumer });
      await new Promise((resolveWait) => setTimeout(resolveWait, 250));
      let definition: unknown;
      for (let attempt = 0; attempt < 80; attempt++) {
        definition = await server.command('definition', {
          file: consumer,
          line: 3,
          offset: (source.split('\n')[2]?.indexOf('Greet') ?? 0) + 1,
        });
        if ((JSON.stringify(definition) ?? '').includes('main.baml')) break;
        await new Promise((resolveWait) => setTimeout(resolveWait, 25));
      }
      expect(JSON.stringify(definition) ?? '').toContain('main.baml');
      expect(JSON.stringify(definition) ?? '').not.toContain('__virtual__');
      const references = await server.command('references', {
        file: consumer,
        line: 3,
        offset: 27,
      });
      expect(JSON.stringify(references)).toContain('consumer.ts');
      expect(JSON.stringify(references)).toContain('main.baml');
      expect(JSON.stringify(references)).not.toContain('__virtual__');
      const rename = await server.command('rename', {
        file: consumer,
        findInComments: false,
        findInStrings: false,
        line: 3,
        offset: 27,
      });
      expect(JSON.stringify(rename)).toContain('main.baml');
      const hover = await server.command('quickinfo', {
        file: consumer,
        line: 3,
        offset: 27,
      });
      expect(JSON.stringify(hover)).toContain('Greet docs');
      await server.command('open', {
        file: baml,
        fileContent:
          '/// Person docs\nclass Person { name string }\n/// Unsaved Greet docs\nfunction Greet(p: Person) -> string { "hi" }\n',
      });
      let unsavedHover: unknown;
      for (let attempt = 0; attempt < 80; attempt++) {
        unsavedHover = await server.command('quickinfo', {
          file: consumer,
          line: 3,
          offset: 27,
        });
        if ((JSON.stringify(unsavedHover) ?? '').includes('Unsaved Greet docs'))
          break;
        await new Promise((resolveWait) => setTimeout(resolveWait, 25));
      }
      expect(JSON.stringify(unsavedHover)).toContain('Unsaved Greet docs');
      const completions = await server.command('completionInfo', {
        file: consumer,
        line: 3,
        offset: 27,
      });
      expect(JSON.stringify(completions)).toContain('Greet');
      const diagnostics = await server.command('semanticDiagnosticsSync', {
        file: consumer,
      });
      expect(diagnostics).toEqual([]);

      // A brand-new `.baml` buffer: the editor opens it, it is never written
      // to disk, and nothing imports it. tsserver's own `readDirectory` is
      // backed by the process filesystem so it cannot see the buffer, and no
      // directory stamp moves — only the editor's open-file registry knows it
      // exists. The compiler must see what the user sees.
      const unsaved = join(directory, 'unsaved.baml');
      await server.command('open', {
        file: unsaved,
        fileContent:
          'class Unsaved { note string }\nfunction Describe(u: Unsaved) -> string { "hi" }\n',
      });
      expect(existsSync(unsaved)).toBe(false);
      const specifierOffset =
        (source.split('\n')[0]?.indexOf("'./main.baml'") ?? 0) + 2;
      const specifierCompletions = async () =>
        JSON.stringify(
          await server.command('completionInfo', {
            file: consumer,
            line: 1,
            offset: specifierOffset,
          }),
        );
      let specifiers = '';
      for (let attempt = 0; attempt < 200; attempt++) {
        specifiers = await specifierCompletions();
        if (specifiers.includes('unsaved.baml')) break;
        await new Promise((resolveWait) => setTimeout(resolveWait, 50));
      }
      expect(specifiers).toContain('unsaved.baml');
      // Still nothing on disk: discovery came from the host, not from a save.
      expect(existsSync(unsaved)).toBe(false);
      // And it stays. The per-request deletion sweep reads existence from the
      // same host view, so a buffer with no disk entry is not mistaken for a
      // file that was deleted right after discovery added it.
      await server.command('semanticDiagnosticsSync', { file: consumer });
      expect(await specifierCompletions()).toContain('unsaved.baml');
    } finally {
      server.close();
    }
  }, 600_000);

  it('serves dependency and workspace-sibling .baml imports', async () => {
    const directory = join(
      await realpath(tmpdir()),
      `baml-tsserver-dep-${process.pid}-${Date.now()}`,
    );
    await mkdir(directory);
    const environment = cleanEnvironment();
    await installPackedToolchain(directory, environment);
    // A dependency shipping its own baml.toml + baml_src inside node_modules,
    // and a pnpm-style workspace sibling reached through a symlinked
    // node_modules entry — both imported with bare specifiers.
    const dependency = join(directory, 'node_modules', 'baml-dep');
    await mkdir(join(dependency, 'baml_src'), { recursive: true });
    await writeFile(
      join(dependency, 'package.json'),
      JSON.stringify({ name: 'baml-dep', type: 'module', version: '1.0.0' }),
    );
    await writeFile(join(dependency, 'baml.toml'), '');
    await writeFile(
      join(dependency, 'baml_src', 'widget.baml'),
      '/// Widget docs\nclass Widget { label string }\nfunction Describe(widget: Widget) -> string { "dep" }\n',
    );
    const shared = join(directory, 'packages', 'shared');
    await mkdir(join(shared, 'baml_src'), { recursive: true });
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
      '/// Shared docs\nclass Shared { note string }\nfunction Summarize(shared: Shared) -> string { "shared" }\n',
    );
    await mkdir(join(directory, 'node_modules', '@shared'), {
      recursive: true,
    });
    await symlink(
      shared,
      join(directory, 'node_modules', '@shared', 'baml'),
      'dir',
    );
    await writeFile(join(directory, 'baml.toml'), '');
    await writeFile(
      join(directory, 'main.baml'),
      'class Local { count int }\nfunction Read(local: Local) -> string { "app" }\n',
    );
    const consumer = join(directory, 'consumer.ts');
    const source =
      "import { b as local } from './main.baml';\n" +
      "import { b as dep, Widget } from 'baml-dep/baml_src/widget.baml';\n" +
      "import { b as shared, Shared } from '@shared/baml/baml_src/shared.baml';\n" +
      'declare const widget: Widget;\n' +
      'declare const value: Shared;\n' +
      'export const result = [local.Read({ count: 1 }), dep.Describe(widget), shared.Summarize(value)] as const;\n';
    await writeFile(consumer, source);
    await writeTsconfig(directory);

    const installedRequire = createRequire(join(directory, 'package.json'));
    const server = new Tsserver(
      installedRequire.resolve('typescript/lib/tsserver.js'),
      directory,
      environment,
    );
    try {
      await server.command('open', { file: consumer, fileContent: source });
      await server.command('semanticDiagnosticsSync', { file: consumer });
      // The dependency import resolves through Node to the physical file in
      // node_modules, owned by the dependency's own session.
      const widgetLine = 4;
      const widgetOffset = (source.split('\n')[3]?.indexOf('Widget') ?? 0) + 1;
      let widgetDefinition = '';
      for (let attempt = 0; attempt < 200; attempt++) {
        widgetDefinition = JSON.stringify(
          await server.command('definition', {
            file: consumer,
            line: widgetLine,
            offset: widgetOffset,
          }),
        );
        if (widgetDefinition.includes('widget.baml')) break;
        await new Promise((resolveWait) => setTimeout(resolveWait, 50));
      }
      expect(widgetDefinition).toContain('baml-dep/baml_src/widget.baml');
      expect(widgetDefinition).not.toContain('__virtual__');
      // The workspace sibling resolves through the symlink to its real path.
      const sharedLine = 5;
      const sharedOffset = (source.split('\n')[4]?.indexOf('Shared') ?? 0) + 1;
      let sharedDefinition = '';
      for (let attempt = 0; attempt < 200; attempt++) {
        sharedDefinition = JSON.stringify(
          await server.command('definition', {
            file: consumer,
            line: sharedLine,
            offset: sharedOffset,
          }),
        );
        if (sharedDefinition.includes('shared.baml')) break;
        await new Promise((resolveWait) => setTimeout(resolveWait, 50));
      }
      expect(sharedDefinition).toContain(
        'packages/shared/baml_src/shared.baml',
      );
      expect(sharedDefinition).not.toContain('@shared');
      expect(sharedDefinition).not.toContain('__virtual__');
      // No silent empty modules: every import typed, no plugin errors.
      const diagnostics = await server.command('semanticDiagnosticsSync', {
        file: consumer,
      });
      expect(diagnostics).toEqual([]);
    } finally {
      server.close();
    }
  }, 600_000);
});

class Tsserver {
  readonly process;
  private sequence = 0;
  #pending = new Map<number, (body: unknown) => void>();
  #buffer = Buffer.alloc(0);

  constructor(serverPath: string, cwd: string, env: NodeJS.ProcessEnv) {
    this.process = spawn(
      process.execPath,
      [
        serverPath,
        '--pluginProbeLocations',
        join(cwd, 'node_modules'),
        '--allowLocalPluginLoads',
        '--logVerbosity',
        'verbose',
        '--logFile',
        join(cwd, 'tsserver.log'),
      ],
      { cwd, env, stdio: ['pipe', 'pipe', 'pipe'] },
    );
    this.process.stdout.on('data', (chunk: Buffer) => {
      this.#buffer = Buffer.concat([this.#buffer, chunk]);
      this.#drain();
    });
  }

  command(command: string, args: object): Promise<unknown> {
    this.sequence += 1;
    const seq = this.sequence;
    const message = JSON.stringify({
      arguments: args,
      command,
      seq,
      type: 'request',
    });
    this.process.stdin.write(`${message}\n`);
    return new Promise((resolveResponse, rejectResponse) => {
      const timeout = setTimeout(
        () => rejectResponse(new Error(`tsserver timed out: ${command}`)),
        10_000,
      );
      this.#pending.set(seq, (body) => {
        clearTimeout(timeout);
        resolveResponse(body);
      });
    });
  }

  close(): void {
    this.process.kill();
  }

  #drain(): void {
    while (true) {
      const headerEnd = this.#buffer.indexOf('\r\n\r\n');
      if (headerEnd < 0) return;
      const header = this.#buffer.subarray(0, headerEnd).toString();
      const length = Number(/Content-Length: (\d+)/i.exec(header)?.[1]);
      const bodyStart = headerEnd + 4;
      if (!Number.isFinite(length) || this.#buffer.length < bodyStart + length)
        return;
      const message = JSON.parse(
        this.#buffer.subarray(bodyStart, bodyStart + length).toString(),
      ) as {
        body?: unknown;
        message?: string;
        request_seq?: number;
        success?: boolean;
        type: string;
      };
      this.#buffer = this.#buffer.subarray(bodyStart + length);
      if (message.type === 'response' && message.request_seq) {
        const resolveResponse = this.#pending.get(message.request_seq);
        this.#pending.delete(message.request_seq);
        resolveResponse?.(
          message.success === false
            ? { error: message.message, success: false }
            : message.body,
        );
      }
    }
  }
}

async function run(
  command: string,
  args: string[],
  cwd: string,
  env: NodeJS.ProcessEnv,
): Promise<void> {
  await new Promise<void>((resolveRun, rejectRun) => {
    const child = spawn(command, args, {
      cwd,
      env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let output = '';
    child.stdout.on('data', (chunk) => {
      output += String(chunk);
    });
    child.stderr.on('data', (chunk) => {
      output += String(chunk);
    });
    child.once('error', rejectRun);
    child.once('exit', (code) => {
      if (code === 0) resolveRun();
      else rejectRun(new Error(`${command} exited with ${code}: ${output}`));
    });
  });
}
