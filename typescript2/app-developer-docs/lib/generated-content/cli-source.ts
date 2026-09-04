import {
  canonicalJson,
  type JsonValue,
  sha256,
} from '@/lib/generated-content/json';
import type { CliArtifactPayload } from '@/lib/generated-content/schemas';

export function canonicalCliSource(
  rawHelp: CliArtifactPayload['raw_help'],
): string {
  const deterministicInputs: JsonValue = [...rawHelp]
    .sort((left, right) =>
      left.command_path.join('\0').localeCompare(right.command_path.join('\0')),
    )
    .map((entry) => ({
      command_path: entry.command_path,
      invocation: entry.invocation,
      text: entry.text,
    }));
  return canonicalJson(deterministicInputs);
}

export function hashCliSource(rawHelp: CliArtifactPayload['raw_help']): string {
  return sha256(canonicalCliSource(rawHelp));
}

export function verifyRawCliHelp(
  rawHelp: CliArtifactPayload['raw_help'],
): void {
  const seenPaths = new Set<string>();
  for (const entry of rawHelp) {
    const pathKey = entry.command_path.join('\0');
    if (seenPaths.has(pathKey)) {
      throw new Error(
        `Duplicate raw CLI help entry: ${entry.command_path.join(' ')}.`,
      );
    }
    seenPaths.add(pathKey);
    if (sha256(entry.text) !== entry.sha256) {
      throw new Error(
        `Raw CLI help hash mismatch: ${entry.command_path.join(' ')}.`,
      );
    }
  }
}
