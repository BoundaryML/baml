import { describe, expect, it } from 'vitest';

import {
  BAML_PLAYGROUND_PROTOCOL_MAX,
  BAML_PLAYGROUND_PROTOCOL_MIN,
  PLAYGROUND_SOURCE_POSITION_ENCODING,
  editorPositionToPlaygroundPosition,
  monacoPositionToPlaygroundPosition,
  parseSourcePositionEncoding,
  playgroundPositionToMonacoPosition,
  playgroundSourceRangeToEditorRange,
  isPlaygroundProtocolCompatible,
} from './protocol';

describe('playground protocol compatibility', () => {
  it('allows new clients to consume both legacy and fixed-position servers', () => {
    expect(BAML_PLAYGROUND_PROTOCOL_MIN).toBe(2);
    expect(BAML_PLAYGROUND_PROTOCOL_MAX).toBe(3);
    expect(isPlaygroundProtocolCompatible(2, 2)).toBe(true);
    expect(isPlaygroundProtocolCompatible(3, 3)).toBe(true);
  });

  it('rejects unrecognized future servers and servers requiring a newer client', () => {
    expect(isPlaygroundProtocolCompatible(4, 3)).toBe(false);
    expect(isPlaygroundProtocolCompatible(3, 4)).toBe(false);
  });
});

describe('playground source position capability', () => {
  it('accepts only the fixed UTF-16 contract', () => {
    expect(parseSourcePositionEncoding(PLAYGROUND_SOURCE_POSITION_ENCODING)).toBe(
      PLAYGROUND_SOURCE_POSITION_ENCODING,
    );
    expect(parseSourcePositionEncoding(undefined)).toBeUndefined();
    expect(parseSourcePositionEncoding('utf8-zero-based-v1')).toBeUndefined();
  });

  it('uses direct zero-based positions only when advertised', () => {
    const source = {
      line: 3,
      column: 4,
      endLine: 5,
      endColumn: 8,
    };

    expect(
      playgroundSourceRangeToEditorRange(
        source,
        PLAYGROUND_SOURCE_POSITION_ENCODING,
      ),
    ).toEqual({
      start: { line: 3, character: 4 },
      end: { line: 5, character: 8 },
    });

    // An old server omits the capability. Preserve the VS Code extension
    // host's existing graph-navigation conversion for that pairing.
    expect(playgroundSourceRangeToEditorRange(source, undefined)).toEqual({
      start: { line: 2, character: 4 },
      end: { line: 4, character: 8 },
    });
  });

  it('uses a point range unless both end coordinates are present', () => {
    expect(
      playgroundSourceRangeToEditorRange(
        { line: 2, column: 3, endLine: 4 },
        PLAYGROUND_SOURCE_POSITION_ENCODING,
      ),
    ).toEqual({
      start: { line: 2, character: 3 },
      end: { line: 2, character: 3 },
    });
  });
});

describe('playground editor adapters', () => {
  it('converts Monaco one-based positions to fixed zero-based UTF-16 coordinates', () => {
    // JavaScript string length is measured in UTF-16 code units: accent = 1,
    // CJK = 1, and the emoji surrogate pair = 2.
    const unicodeColumn = 'e\u0301界😀'.length;
    expect(unicodeColumn).toBe(5);
    expect(
      monacoPositionToPlaygroundPosition({
        lineNumber: 2,
        column: unicodeColumn + 1,
      }),
    ).toEqual({ line: 1, column: unicodeColumn });
  });

  it('keeps VS Code cursor columns in UTF-16 code units', () => {
    const utf16Column = 'ASCII-é-界-😀'.length;
    expect(
      editorPositionToPlaygroundPosition({ line: 7, character: utf16Column }),
    ).toEqual({ line: 7, column: utf16Column });
  });

  it('converts advertised source positions back to Monaco one-based coordinates', () => {
    expect(
      playgroundPositionToMonacoPosition(
        { line: 0, column: 5 },
        PLAYGROUND_SOURCE_POSITION_ENCODING,
      ),
    ).toEqual({ lineNumber: 1, column: 6 });

    expect(
      playgroundPositionToMonacoPosition({ line: 1, column: 5 }, undefined),
    ).toEqual({ lineNumber: 1, column: 6 });
  });
});
