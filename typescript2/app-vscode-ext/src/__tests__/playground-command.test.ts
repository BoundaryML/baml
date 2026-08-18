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
    platform: 'linux',
    projectPath,
    shell: '/bin/zsh',
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
      command: "'/opt/BAML CLI/baml' playground",
    });
  });

  it('passes directories through the supported --project flag', () => {
    const directory = temporaryDirectory();

    expect(commandFor(directory)).toEqual({
      command: `'/opt/BAML CLI/baml' playground --project '${directory}'`,
      cwd: directory,
    });
  });

  it('preserves --file for standalone BAML files', () => {
    const directory = temporaryDirectory();
    const file = path.join(directory, 'main.baml');
    fs.writeFileSync(file, 'function Hello() -> string { "hello" }');

    expect(commandFor(file)).toEqual({
      command: `'/opt/BAML CLI/baml' playground --file '${file}'`,
      cwd: directory,
    });
  });

  it('uses --project for missing paths so the CLI can report the error', () => {
    const missing = path.join(temporaryDirectory(), 'missing project');

    expect(commandFor(missing)).toEqual({
      command: `'/opt/BAML CLI/baml' playground --project '${missing}'`,
    });
  });

  it('keeps PowerShell invocation and quoting semantics', () => {
    expect(
      playgroundCommandForPath({
        platform: 'win32',
        projectPath: "C:\\Users\\Sam's project",
        shell: 'C:\\Program Files\\PowerShell\\7\\pwsh.exe',
        wrapperPath: 'C:\\Program Files\\BAML\\baml.exe',
      }),
    ).toEqual({
      command:
        "& 'C:\\Program Files\\BAML\\baml.exe' playground --project 'C:\\Users\\Sam''s project'",
    });
  });
});
