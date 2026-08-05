import { spawn } from 'node:child_process';
import { mkdir, readdir, realpath, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const nativeBridge = process.env.BAML_TOOLING_BRIDGE_PATH;
const workspace = resolve(import.meta.dirname, '../..');
const repository = resolve(workspace, '..');
const require = createRequire(import.meta.url);

describe.runIf(nativeBridge)('packed tsserver protocol', () => {
  it('returns physical BAML locations for editor protocol requests', async () => {
    const directory = join(
      await realpath(tmpdir()),
      `baml-tsserver-${process.pid}-${Date.now()}`,
    );
    await mkdir(directory);
    await run(
      'pnpm',
      ['--filter', '@boundaryml/baml-tooling', 'build'],
      workspace,
    );
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
    );
    await run(
      'npm',
      [
        'pack',
        resolve(repository, 'baml_language/sdks/typescript/bridge_tooling'),
        '--pack-destination',
        directory,
      ],
      workspace,
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
        '--omit=optional',
        '--no-package-lock',
        ...archives,
      ],
      directory,
    );
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
    await writeFile(
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

    const server = new Tsserver(
      require.resolve('typescript/lib/tsserver.js'),
      directory,
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
    } finally {
      server.close();
    }
  }, 90_000);
});

class Tsserver {
  readonly process;
  private sequence = 0;
  #pending = new Map<number, (body: unknown) => void>();
  #buffer = Buffer.alloc(0);

  constructor(serverPath: string, cwd: string) {
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
      { cwd, stdio: ['pipe', 'pipe', 'pipe'] },
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
): Promise<void> {
  await new Promise<void>((resolveRun, rejectRun) => {
    const child = spawn(command, args, {
      cwd,
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
