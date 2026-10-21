import assert from 'node:assert/strict';
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  findPackagedVsix,
  installLocalVscodeExtension,
} from './install-vscode-extension.mjs';

async function withTemporaryWorkspace(run) {
  const root = await mkdtemp(path.join(tmpdir(), 'baml-vscode-install-'));
  try {
    await run(root);
  } finally {
    await rm(root, { force: true, recursive: true });
  }
}

test('findPackagedVsix requires exactly one VSIX', async () => {
  await withTemporaryWorkspace(async (root) => {
    await assert.rejects(findPackagedVsix(root), /found none/);

    await writeFile(path.join(root, 'first.vsix'), 'first');
    assert.equal(await findPackagedVsix(root), path.join(root, 'first.vsix'));

    await writeFile(path.join(root, 'second.vsix'), 'second');
    await assert.rejects(
      findPackagedVsix(root),
      /found first\.vsix, second\.vsix/,
    );
  });
});

test('installLocalVscodeExtension packages and force-installs the generated VSIX', async () => {
  await withTemporaryWorkspace(async (root) => {
    const extensionDirectory = path.join(root, 'app-vscode-ext');
    const vsix = path.join(extensionDirectory, 'baml-language-0.16.0.vsix');
    await mkdir(extensionDirectory);
    await writeFile(vsix, 'extension');
    const calls = [];
    const run = async (command, args, options) =>
      calls.push({ args, command, options });

    assert.equal(
      await installLocalVscodeExtension({ platform: 'linux', root, run }),
      vsix,
    );
    assert.deepEqual(calls, [
      {
        args: ['--version'],
        command: 'code',
        options: { cwd: root, stdio: 'ignore' },
      },
      {
        args: ['run', 'vscode:package'],
        command: 'pnpm',
        options: { cwd: root },
      },
      {
        args: ['--install-extension', vsix, '--force'],
        command: 'code',
        options: { cwd: root },
      },
    ]);
  });
});

test('installLocalVscodeExtension launches Windows command shims through a shell', async () => {
  await withTemporaryWorkspace(async (root) => {
    const extensionDirectory = path.join(root, 'app-vscode-ext');
    const vsix = path.join(extensionDirectory, 'baml-language-0.16.0.vsix');
    await mkdir(extensionDirectory);
    await writeFile(vsix, 'extension');
    const calls = [];
    const run = async (command, args, options) =>
      calls.push({ args, command, options });

    await installLocalVscodeExtension({ platform: 'win32', root, run });
    assert.deepEqual(calls, [
      {
        args: ['--version'],
        command: 'code.cmd',
        options: { cwd: root, shell: true, stdio: 'ignore' },
      },
      {
        args: ['run', 'vscode:package'],
        command: 'pnpm.cmd',
        options: { cwd: root, shell: true },
      },
      {
        args: ['--install-extension', vsix, '--force'],
        command: 'code.cmd',
        options: { cwd: root, shell: true },
      },
    ]);
  });
});

test('installLocalVscodeExtension fails before packaging when the code CLI is missing', async () => {
  const calls = [];
  const run = async (command, args) => {
    calls.push([command, args]);
    const error = new Error('not found');
    error.code = 'ENOENT';
    throw error;
  };

  await assert.rejects(
    installLocalVscodeExtension({
      platform: 'linux',
      root: '/repo/typescript2',
      run,
    }),
    /The VS Code CLI 'code' was not found on PATH/,
  );
  assert.deepEqual(calls, [['code', ['--version']]]);
});
