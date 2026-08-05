import { execFile } from 'node:child_process';
import { mkdtemp, readdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { promisify } from 'node:util';
import { describe, expect, it } from 'vitest';

const execFileAsync = promisify(execFile);
const workspace = resolve(import.meta.dirname, '../..');
const repository = resolve(workspace, '..');

describe('packed install', () => {
  it('loads the ESM/CJS tooling and CJS tsserver plugin without undeclared dependencies', async () => {
    await execFileAsync(
      'pnpm',
      ['--filter', '@boundaryml/baml-tooling', 'build'],
      { cwd: workspace },
    );
    const directory = await mkdtemp(resolve(tmpdir(), 'baml-packed-'));
    await execFileAsync(
      'pnpm',
      [
        '--dir',
        resolve(workspace, 'pkg-baml-tooling'),
        'pack',
        '--pack-destination',
        directory,
      ],
      { cwd: workspace },
    );
    await execFileAsync(
      'npm',
      [
        'pack',
        resolve(repository, 'baml_language/sdks/typescript/bridge_tooling'),
        '--pack-destination',
        directory,
      ],
      { cwd: workspace },
    );
    const archives = (await readdir(directory))
      .filter((name) => name.endsWith('.tgz'))
      .map((name) => resolve(directory, name));
    expect(archives).toHaveLength(2);
    await writeFile(
      resolve(directory, 'package.json'),
      JSON.stringify({ private: true, type: 'module' }),
    );
    await execFileAsync(
      'npm',
      [
        'install',
        '--ignore-scripts',
        '--omit=optional',
        '--no-package-lock',
        ...archives,
      ],
      { cwd: directory },
    );
    await execFileAsync(
      process.execPath,
      [
        '--input-type=module',
        '-e',
        "const tooling = await import('@boundaryml/baml-tooling'); if (typeof tooling.loadProject !== 'function') process.exit(1);",
      ],
      { cwd: directory },
    );
    const { stdout } = await execFileAsync(
      process.execPath,
      [
        '-e',
        "const plugin = require('@boundaryml/baml-tooling/typescript-plugin'); process.stdout.write(typeof plugin);",
      ],
      { cwd: directory },
    );
    expect(stdout).toBe('function');

    await writeFile(resolve(directory, 'baml.toml'), '');
    await writeFile(resolve(directory, 'main.baml'), 'class Person {}\n');
    const environment = { ...process.env };
    delete environment.BAML_TOOLING_BRIDGE_PATH;
    delete environment.INIT_CWD;
    await execFileAsync(
      process.execPath,
      [
        '--input-type=module',
        '-e',
        "const { loadProject } = await import('@boundaryml/baml-tooling'); const project = await loadProject({ backend: 'native', cwd: process.cwd() }); if (project.check().diagnostics.length) process.exit(1); project.dispose();",
      ],
      { cwd: directory, env: environment },
    );
  }, 60_000);
});
