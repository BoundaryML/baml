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
  for (const char of text) {
    const bytes = new TextEncoder().encode(char).length;
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
