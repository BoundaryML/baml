import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { playgroundCommandForPath } from '../playground-command';

const temporaryDirectories: string[] = [];

function temporaryDirectory(): string {
  const directory = fs.mkdtempSync(
    path.join(os.tmpdir(), 'baml-vscode-playground-'),
  );
  temporaryDirectories.push(directory);
  return directory;
}

function commandFor(projectPath?: string) {
  return playgroundCommandForPath({
    projectPath,
    wrapperPath: '/opt/BAML CLI/baml',
  });
}

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    fs.rmSync(directory, { force: true, recursive: true });
  }
});

describe('playgroundCommandForPath', () => {
  it('launches the playground without a path when no project is selected', () => {
    expect(commandFor()).toEqual({
      args: ['playground'],
      executable: '/opt/BAML CLI/baml',
    });
  });

  it('passes directories through the supported --project flag', () => {
    const directory = temporaryDirectory();

    expect(commandFor(directory)).toEqual({
      args: ['playground', '--project', directory],
      cwd: directory,
      executable: '/opt/BAML CLI/baml',
    });
  });

  it('preserves --file for standalone BAML files', () => {
    const directory = temporaryDirectory();
    const file = path.join(directory, 'main.baml');
    fs.writeFileSync(file, 'function Hello() -> string { "hello" }');

    expect(commandFor(file)).toEqual({
      args: ['playground', '--file', file],
      cwd: directory,
      executable: '/opt/BAML CLI/baml',
    });
  });

  it('uses --project for missing paths so the CLI can report the error', () => {
    const missing = path.join(temporaryDirectory(), 'missing project');

    expect(commandFor(missing)).toEqual({
      args: ['playground', '--project', missing],
      executable: '/opt/BAML CLI/baml',
    });
  });

  it('preserves shell metacharacters as literal argv values', () => {
    const projectPath =
      "C:\\repo\\$(touch injected)\\%TEMP%\\!TEMP!\\Sam's project";

    expect(commandFor(projectPath)).toEqual({
      args: ['playground', '--project', projectPath],
      executable: '/opt/BAML CLI/baml',
    });
  });
});
