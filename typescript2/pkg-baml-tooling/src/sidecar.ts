import { basename } from 'node:path';

export function isBamlSidecar(path: string): boolean {
  const name = basename(path);
  return name.endsWith('.baml.ts') || name.endsWith('.baml.d.ts');
}

export function sidecarPath(source: string, declarationsOnly = true): string {
  return declarationsOnly ? `${source}.d.ts` : `${source}.ts`;
}

export function sidecarFingerprint(text: string): string | undefined {
  return /^\/\/ baml-fingerprint: ([a-f0-9]+)$/m.exec(text)?.[1];
}
