import type { Location, Segment, SegmentMap } from './generated/tooling.js';

export interface GeneratedRef {
  symbolId: string;
  signatureId?: string;
  segment: Segment;
}

export function generatedToSource(
  map: SegmentMap,
  offsetUtf16: number,
): (Location & GeneratedRef) | undefined {
  void map;
  void offsetUtf16;
  throw new Error('not implemented');
}

export function sourceToGenerated(
  map: SegmentMap,
  path: string,
  offsetUtf8: number,
): Segment[] {
  void map;
  void path;
  void offsetUtf8;
  throw new Error('not implemented');
}

export function assertMapHashes(
  map: SegmentMap,
  hashes: ReadonlyMap<string, string>,
): void {
  void map;
  void hashes;
  throw new Error('not implemented');
}
