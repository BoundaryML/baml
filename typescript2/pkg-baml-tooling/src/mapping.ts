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
  let low = 0;
  let high = map.segments.length - 1;
  while (low <= high) {
    const middle = (low + high) >>> 1;
    const segment = map.segments[middle];
    if (!segment) return undefined;
    if (offsetUtf16 < segment.genStartUtf16) high = middle - 1;
    else if (offsetUtf16 >= segment.genStartUtf16 + segment.genLengthUtf16)
      low = middle + 1;
    else {
      const path = map.sources[segment.sourceFile];
      if (!path) return undefined;
      return {
        lengthUtf8: segment.sourceLengthUtf8,
        path,
        segment,
        signatureId: segment.signatureId || undefined,
        startUtf8: segment.sourceStartUtf8,
        symbolId: segment.symbolId,
      };
    }
  }
  return undefined;
}

export function sourceToGenerated(
  map: SegmentMap,
  path: string,
  offsetUtf8: number,
): Segment[] {
  const sourceFile = map.sources.indexOf(path);
  if (sourceFile < 0) return [];
  return map.segments.filter(
    (segment) =>
      segment.sourceFile === sourceFile &&
      offsetUtf8 >= segment.sourceStartUtf8 &&
      offsetUtf8 < segment.sourceStartUtf8 + segment.sourceLengthUtf8,
  );
}

export function assertMapHashes(
  map: SegmentMap,
  hashes: ReadonlyMap<string, string>,
): void {
  map.sources.forEach((source, index) => {
    if (hashes.get(source) !== map.sourceHashes[index])
      throw new Error(`stale BAML segment map for ${source}`);
  });
}
