import { spawn } from 'node:child_process';
import { constants } from 'node:fs';
import { access } from 'node:fs/promises';
import path from 'node:path';

export interface BamlBinaryIdentity {
  productVersion: string;
  rawVersionOutput: string;
  wrapperVersion: string;
}

export interface BamlInvocationResult {
  arguments: string[];
  stderr: string;
  stdout: string;
}

export async function validateBamlBinaryPath(
  binaryPath: string,
): Promise<string> {
  if (!path.isAbsolute(binaryPath)) {
    throw new Error('--baml-bin must be an absolute path.');
  }
  await access(binaryPath, constants.X_OK);
  return binaryPath;
}

export async function invokeBaml(
  binaryPath: string,
  argumentsToPass: readonly string[],
): Promise<BamlInvocationResult> {
  await validateBamlBinaryPath(binaryPath);
  return new Promise((resolve, reject) => {
    const child = spawn(binaryPath, argumentsToPass, {
      env: {
        ...process.env,
        CLICOLOR: '0',
        CLICOLOR_FORCE: '0',
        NO_COLOR: '1',
        TERM: 'dumb',
      },
      shell: false,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    child.stdout.on('data', (chunk: Buffer) => stdout.push(chunk));
    child.stderr.on('data', (chunk: Buffer) => stderr.push(chunk));
    child.on('error', reject);
    child.on('close', (exitCode) => {
      const result = {
        arguments: [...argumentsToPass],
        stderr: Buffer.concat(stderr).toString('utf8'),
        stdout: Buffer.concat(stdout).toString('utf8'),
      };
      if (exitCode !== 0) {
        reject(
          new Error(
            `Selected baml binary exited with status ${exitCode ?? 'unknown'} for ${argumentsToPass.join(' ')}.\n${result.stderr || result.stdout}`,
          ),
        );
        return;
      }
      resolve(result);
    });
  });
}

export async function readBamlBinaryIdentity(
  binaryPath: string,
): Promise<BamlBinaryIdentity> {
  const result = await invokeBaml(binaryPath, ['--version']);
  const wrapperMatch = /^baml wrapper (\S+)$/m.exec(result.stdout);
  const toolchainMatch = /^baml toolchain (\S+)(?: \([^\n]+\))?$/m.exec(
    result.stdout,
  );
  if (!wrapperMatch || !toolchainMatch) {
    throw new Error(
      'The selected baml binary did not expose the required wrapper and toolchain identities.',
    );
  }
  return {
    productVersion: toolchainMatch[1],
    rawVersionOutput: result.stdout,
    wrapperVersion: wrapperMatch[1],
  };
}
