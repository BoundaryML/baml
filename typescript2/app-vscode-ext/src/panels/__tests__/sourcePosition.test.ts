import { describe, expect, it } from 'vitest';

import {
  FIXED_SOURCE_POSITION_ENCODING,
  playgroundLineToEditorLine,
} from '../sourcePosition';

describe('playground source position compatibility', () => {
  it('uses advertised zero-based UTF-16 lines directly', () => {
    expect(playgroundLineToEditorLine(4, FIXED_SOURCE_POSITION_ENCODING)).toBe(4);
  });

  it('preserves the legacy subtraction when capability is absent or unknown', () => {
    expect(playgroundLineToEditorLine(4)).toBe(3);
    expect(playgroundLineToEditorLine(4, 'future-encoding')).toBe(3);
    expect(playgroundLineToEditorLine(0)).toBe(0);
  });
});
