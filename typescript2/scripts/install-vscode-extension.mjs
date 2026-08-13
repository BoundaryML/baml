#!/usr/bin/env node

import { spawn } from 'node:child_process';
import { readdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const workspaceRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..',
);

export async function findPackagedVsix(extensionDirectory) {
  let entries;
  try {
    entries = await readdir(extensionDirectory, { withFileTypes: true });
  } catch (error) {
    if (error?.code === 'ENOENT') {
      throw new Error(
        `Expected one packaged VSIX in ${extensionDirectory}, but the directory does not exist.`,
      );
    }
    throw error;
  }

  const candidates = entries
    .filter((entry) => entry.isFile() && entry.name.endsWith('.vsix'))
    .map((entry) => entry.name)
    .sort();
  if (candidates.length !== 1) {
    const found = candidates.length === 0 ? 'none' : candidates.join(', ');
    throw new Error(
      `Expected one packaged VSIX in ${extensionDirectory}, found ${found}.`,
    );
  }

  return path.join(extensionDirectory, candidates[0]);
}

export function runCommand(command, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      ...options,
      stdio: options.stdio ?? 'inherit',
    });
    child.on('error', reject);
    child.on('close', (code, signal) => {
      if (code === 0) {
        resolve();
        return;
      }
      const outcome =
        signal === null ? `exit code ${code}` : `signal ${signal}`;
      reject(new Error(`${command} ${args.join(' ')} failed with ${outcome}.`));
    });
  });
}

export async function installLocalVscodeExtension({
  root = workspaceRoot,
  platform = process.platform,
  run = runCommand,
} = {}) {
  const codeCommand = platform === 'win32' ? 'code.cmd' : 'code';
  const pnpmCommand = platform === 'win32' ? 'pnpm.cmd' : 'pnpm';

  try {
    await run(codeCommand, ['--version'], { cwd: root, stdio: 'ignore' });
  } catch (error) {
    if (error?.code === 'ENOENT') {
      throw new Error(
        "The VS Code CLI 'code' was not found on PATH. In VS Code, run 'Shell Command: Install code command in PATH', then retry.",
      );
    }
    throw error;
  }

  await run(pnpmCommand, ['run', 'vscode:package'], { cwd: root });
  const vsix = await findPackagedVsix(path.join(root, 'app-vscode-ext'));
  await run(codeCommand, ['--install-extension', vsix, '--force'], {
    cwd: root,
  });
  return vsix;
}

const isMain =
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  installLocalVscodeExtension().catch((error) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}
