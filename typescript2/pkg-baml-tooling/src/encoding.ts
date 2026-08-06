export interface LineIndex {
  readonly text: string;
  readonly utf8ToUtf16: readonly number[];
  readonly utf16ToUtf8: readonly number[];
}

export function buildLineIndex(text: string): LineIndex {
  void text;
  throw new Error('not implemented');
}

export function utf8OffsetToUtf16(index: LineIndex, offset: number): number {
  void index;
  void offset;
  throw new Error('not implemented');
}

export function utf16OffsetToUtf8(index: LineIndex, offset: number): number {
  void index;
  void offset;
  throw new Error('not implemented');
}
