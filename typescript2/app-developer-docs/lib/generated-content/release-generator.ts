import { spawn } from 'node:child_process';

import { z } from 'zod';

import { readStandardPackageAllowlist } from '@/lib/generated-content/allowlist';
import { readBamlBinaryIdentity } from '@/lib/generated-content/baml-binary';
import {
  type CliPublicationInput,
  generateCliPublicationInput,
} from '@/lib/generated-content/cli-generator';
import {
  generatePackagePublicationInput,
  type PackagePublicationInput,
} from '@/lib/generated-content/package-generator';

const sourceCommitSchema = z.string().regex(/^[0-9a-f]{40}$/);
const releasedAtSchema = z.string().datetime({ offset: true });

export interface CompleteReleaseGenerationInput {
  bamlBinary: string;
  releasedAt: string;
  sourceCommit: string;
}

export interface CompleteReleasePublicationInput {
  cli: CliPublicationInput;
  generatedAt: string;
  generatorVersion: string;
  packages: PackagePublicationInput[];
  releasedAt: string;
  sourceCommit: string;
  version: string;
  wrapperVersion: string;
}

async function readGitRevision(): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn('git', ['rev-parse', 'HEAD'], {
      shell: false,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    child.stdout.on('data', (chunk: Buffer) => stdout.push(chunk));
    child.stderr.on('data', (chunk: Buffer) => stderr.push(chunk));
    child.on('error', reject);
    child.on('close', (exitCode) => {
      if (exitCode !== 0) {
        reject(
          new Error(
            `Unable to identify the documentation generator revision: ${Buffer.concat(stderr).toString('utf8')}`,
          ),
        );
        return;
      }
      resolve(
        sourceCommitSchema.parse(Buffer.concat(stdout).toString('utf8').trim()),
      );
    });
  });
}

export async function generateCompleteRelease(
  input: CompleteReleaseGenerationInput,
): Promise<CompleteReleasePublicationInput> {
  const sourceCommit = sourceCommitSchema.parse(input.sourceCommit);
  const releasedAt = releasedAtSchema.parse(input.releasedAt);
  const [identity, packageNames, generatorRevision] = await Promise.all([
    readBamlBinaryIdentity(input.bamlBinary),
    readStandardPackageAllowlist(),
    readGitRevision(),
  ]);
  const [packages, cli] = await Promise.all([
    Promise.all(
      packageNames.map((packageName) =>
        generatePackagePublicationInput(input.bamlBinary, packageName),
      ),
    ),
    generateCliPublicationInput(input.bamlBinary),
  ]);

  if (
    cli.productVersion !== identity.productVersion ||
    cli.wrapperVersion !== identity.wrapperVersion
  ) {
    throw new Error('Selected baml binary identity changed during generation.');
  }

  const releaseRoutes = new Set<string>();
  for (const packageInput of packages) {
    for (const page of packageInput.pages) {
      if (releaseRoutes.has(page.routePath)) {
        throw new Error(
          `Release-wide package route collision: ${page.routePath}.`,
        );
      }
      releaseRoutes.add(page.routePath);
    }
  }

  return {
    cli,
    generatedAt: new Date().toISOString(),
    generatorVersion: generatorRevision,
    packages,
    releasedAt,
    sourceCommit,
    version: identity.productVersion,
    wrapperVersion: identity.wrapperVersion,
  };
}
