import { sha256 } from '@/lib/generated-content/json';

const FQN_SEGMENT_PATTERN = /^[A-Za-z_$][A-Za-z0-9_$]*$/;
const EXPORTED_ID_PATTERN = /^[A-Z]:.+$/;

export interface AnchorInput {
  exportedId: string;
  label: string;
}

export interface MemberAnchor extends AnchorInput {
  anchor: string;
}

function validateFqnSegment(segment: string): void {
  if (!FQN_SEGMENT_PATTERN.test(segment)) {
    throw new Error(
      `Invalid fully qualified name segment: ${JSON.stringify(segment)}.`,
    );
  }
}

export function splitQualifiedName(qualifiedName: string): string[] {
  const segments = qualifiedName.split('.');
  if (
    segments.length === 0 ||
    segments.some((segment) => segment.length === 0)
  ) {
    throw new Error(
      `Invalid fully qualified name: ${JSON.stringify(qualifiedName)}.`,
    );
  }
  for (const segment of segments) {
    validateFqnSegment(segment);
  }
  return segments;
}

export function qualifiedNameToRoutePath(qualifiedName: string): string {
  return splitQualifiedName(qualifiedName).join('/');
}

export function deriveParentQualifiedName(
  qualifiedName: string,
): string | null {
  const segments = splitQualifiedName(qualifiedName);
  return segments.length === 1 ? null : segments.slice(0, -1).join('.');
}

export function qualifyExportedName(
  packageName: string,
  namespace: readonly string[],
  name: string,
): string {
  const segments = [packageName, ...namespace, name];
  for (const segment of segments) {
    validateFqnSegment(segment);
  }
  return segments.join('.');
}

export function createMemberAnchors(
  inputs: readonly AnchorInput[],
): MemberAnchor[] {
  const labelCounts = new Map<string, number>();
  const exportedIds = new Set<string>();

  for (const input of inputs) {
    validateFqnSegment(input.label);
    if (!EXPORTED_ID_PATTERN.test(input.exportedId)) {
      throw new Error(
        `Invalid stable exported ID: ${JSON.stringify(input.exportedId)}.`,
      );
    }
    if (exportedIds.has(input.exportedId)) {
      throw new Error(`Duplicate stable exported ID: ${input.exportedId}.`);
    }
    exportedIds.add(input.exportedId);
    labelCounts.set(input.label, (labelCounts.get(input.label) ?? 0) + 1);
  }

  const anchors = inputs.map((input) => ({
    ...input,
    anchor:
      labelCounts.get(input.label) === 1
        ? input.label
        : `${input.label}-${sha256(input.exportedId).slice(0, 8)}`,
  }));

  const seenAnchors = new Set<string>();
  for (const member of anchors) {
    if (seenAnchors.has(member.anchor)) {
      throw new Error(
        `Deterministic member anchor collision: ${member.anchor}.`,
      );
    }
    seenAnchors.add(member.anchor);
  }

  return anchors;
}
