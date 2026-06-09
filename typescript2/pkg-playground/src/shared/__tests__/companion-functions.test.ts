import { describe, expect, it } from 'vitest';
import { companionFunctionName } from '../companion-functions';

describe('companion-functions', () => {
  it('uses the compiler companion naming convention', () => {
    expect(companionFunctionName('ClassifySentiment', 'render_prompt')).toBe('ClassifySentiment$render_prompt');
    expect(companionFunctionName('ClassifySentiment', 'build_request')).toBe('ClassifySentiment$build_request');
  });
});
