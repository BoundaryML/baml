export interface LineIndex {
  readonly text: string;
  readonly utf8ToUtf16: readonly number[];
  readonly utf16ToUtf8: readonly number[];
}

export function buildLineIndex(text: string): LineIndex {
  const utf8ToUtf16: number[] = [0];
  const utf16ToUtf8: number[] = [0];
  let byte = 0;
  let unit = 0;
  // UTF-8 length is a pure function of the code point; deriving it arithmetically
  // keeps this a single allocation-free pass instead of one TextEncoder
  // allocation per character (this runs on tsserver's thread).
  for (const char of text) {
    const code = char.codePointAt(0) ?? 0;
    const bytes = code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    const units = char.length;
    for (let index = 1; index <= bytes; index++)
      utf8ToUtf16[byte + index] = unit + (index === bytes ? units : 0);
    for (let index = 1; index <= units; index++)
      utf16ToUtf8[unit + index] = byte + (index === units ? bytes : 0);
    byte += bytes;
    unit += units;
  }
  return { text, utf8ToUtf16, utf16ToUtf8 };
}

/**
 * Caches one line index per path, keyed by snapshot text identity. Editor
 * requests translate many locations against the same unchanging snapshot, so
 * rebuilding the index per location (twice per diagnostic) is pure waste.
 */
export class LineIndexCache {
  readonly #entries = new Map<string, LineIndex>();

  forText(path: string, text: string): LineIndex {
    const cached = this.#entries.get(path);
    if (cached && cached.text === text) return cached;
    const index = buildLineIndex(text);
    this.#entries.set(path, index);
    return index;
  }

  delete(path: string): void {
    this.#entries.delete(path);
  }

  clear(): void {
    this.#entries.clear();
  }
}

export function utf8OffsetToUtf16(index: LineIndex, offset: number): number {
  if (offset < 0 || offset >= index.utf8ToUtf16.length)
    throw new RangeError(
      `UTF-8 offset ${offset} is outside the source snapshot`,
    );
  return index.utf8ToUtf16[offset] ?? 0;
}

export function utf16OffsetToUtf8(index: LineIndex, offset: number): number {
  if (offset < 0 || offset >= index.utf16ToUtf8.length)
    throw new RangeError(
      `UTF-16 offset ${offset} is outside the source snapshot`,
    );
  return index.utf16ToUtf8[offset] ?? 0;
}
