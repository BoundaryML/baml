export const FIXED_SOURCE_POSITION_ENCODING = 'utf16-zero-based-v1';

/**
 * The current playground contract is zero-based. Older servers did not
 * advertise their contract, and this extension route historically subtracted
 * one from graph-navigation lines, so preserve that behavior when absent.
 */
export function playgroundLineToEditorLine(
  line: number,
  sourcePositionEncoding?: string,
): number {
  return sourcePositionEncoding === FIXED_SOURCE_POSITION_ENCODING
    ? line
    : Math.max(0, line - 1);
}
