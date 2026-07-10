export const BAML_PLAYGROUND_PROTOCOL_MIN = 2;
export const BAML_PLAYGROUND_PROTOCOL_MAX = 3;

/**
 * Playground source coordinates are independent of the negotiated LSP
 * position encoding. The wire contract uses zero-based lines and UTF-16 code
 * units, matching VS Code's Position API.
 */
export const PLAYGROUND_SOURCE_POSITION_ENCODING = 'utf16-zero-based-v1' as const;

export type SourcePositionEncoding = typeof PLAYGROUND_SOURCE_POSITION_ENCODING;

export interface PlaygroundPosition {
  line: number;
  column: number;
}

export interface EditorPosition {
  line: number;
  character: number;
}

export interface MonacoPosition {
  lineNumber: number;
  column: number;
}

export interface PlaygroundSourceRange extends PlaygroundPosition {
  endLine?: number;
  endColumn?: number;
}

export interface EditorRange {
  start: EditorPosition;
  end: EditorPosition;
}

/** Reject unknown future encodings instead of silently applying this adapter. */
export function parseSourcePositionEncoding(value: unknown): SourcePositionEncoding | undefined {
  return value === PLAYGROUND_SOURCE_POSITION_ENCODING ? value : undefined;
}

/** Convert the VS Code API's zero-based UTF-16 position to playground wire coordinates. */
export function editorPositionToPlaygroundPosition(position: EditorPosition): PlaygroundPosition {
  return { line: position.line, column: position.character };
}

/** Convert Monaco's one-based UTF-16 IPosition to playground wire coordinates. */
export function monacoPositionToPlaygroundPosition(position: MonacoPosition): PlaygroundPosition {
  return {
    line: Math.max(0, position.lineNumber - 1),
    column: Math.max(0, position.column - 1),
  };
}

/**
 * Convert a playground source position to the VS Code API's Position shape.
 *
 * Older servers did not advertise their source-position encoding and the VS
 * Code extension host's graph-navigation path treated source lines as
 * one-based. Preserve that subtraction unless the fixed zero-based UTF-16
 * contract is advertised.
 */
export function playgroundPositionToEditorPosition(
  position: PlaygroundPosition,
  encoding: SourcePositionEncoding | undefined,
): EditorPosition {
  const line = encoding === PLAYGROUND_SOURCE_POSITION_ENCODING
    ? position.line
    : Math.max(0, position.line - 1);
  return { line, character: position.column };
}

/** Convert a playground source range to zero-based UTF-16 editor positions. */
export function playgroundSourceRangeToEditorRange(
  range: PlaygroundSourceRange,
  encoding: SourcePositionEncoding | undefined,
): EditorRange {
  const start = playgroundPositionToEditorPosition(range, encoding);
  if (range.endLine === undefined || range.endColumn === undefined) {
    return { start, end: start };
  }

  return {
    start,
    end: playgroundPositionToEditorPosition(
      {
        line: range.endLine,
        column: range.endColumn,
      },
      encoding,
    ),
  };
}

/** Convert a playground source position to Monaco's one-based UTF-16 IPosition. */
export function playgroundPositionToMonacoPosition(
  position: PlaygroundPosition,
  encoding: SourcePositionEncoding | undefined,
): MonacoPosition {
  const editorPosition = playgroundPositionToEditorPosition(position, encoding);
  return {
    lineNumber: editorPosition.line + 1,
    column: editorPosition.character + 1,
  };
}

export function isPlaygroundProtocolCompatible(
  serverProtocol: number,
  minClientProtocol: number,
): boolean {
  return (
    serverProtocol >= BAML_PLAYGROUND_PROTOCOL_MIN &&
    serverProtocol <= BAML_PLAYGROUND_PROTOCOL_MAX &&
    BAML_PLAYGROUND_PROTOCOL_MAX >= minClientProtocol
  );
}
