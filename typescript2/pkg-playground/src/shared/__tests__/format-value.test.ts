import { describe, it, expect } from 'vitest';
import { formatValue } from '../format-value';

describe('formatValue', () => {
  it('formats primitives', () => {
    expect(formatValue('hello', 'inline-hint')).toBe('"hello"');
    expect(formatValue(42, 'inline-hint')).toBe('42');
    expect(formatValue(true, 'inline-hint')).toBe('true');
    expect(formatValue(false, 'inline-hint')).toBe('false');
    expect(formatValue(null, 'inline-hint')).toBe('null');
    expect(formatValue(undefined, 'inline-hint')).toBe('null');
  });

  it('formats arrays', () => {
    expect(formatValue([1, 2, 3], 'inline-hint')).toBe('[1, 2, 3]');
    expect(formatValue([], 'inline-hint')).toBe('[]');
  });

  it('formats class with $baml type', () => {
    const value = { $baml: { type: 'Person' }, name: 'Alice', age: 30 };
    const result = formatValue(value, 'inline-hint');
    expect(result).toBe('Person { name: "Alice", age: 30 }');
  });

  it('formats handle with PlainHandleDescriptor', () => {
    const value = {
      $baml: { type: '$handle' },
      handle: { handle_key: 42n, handle_type: 0, type_name: 'function_ref' },
    } as Parameters<typeof formatValue>[0];
    expect(formatValue(value, 'inline-hint')).toBe('<handle #42>');
  });

  it('formats media', () => {
    const value = {
      $baml: { type: '$media' },
      media_type: 'image',
      mime_type: 'image/png',
      content_type: 'url',
      url: 'https://example.com/img.png',
    };
    expect(formatValue(value, 'inline-hint')).toBe('<image/png>');
  });

  it('formats prompt_ast', () => {
    const value = { $baml: { type: '$prompt_ast' }, content_type: 'simple', value: {} };
    expect(formatValue(value, 'inline-hint')).toBe('<prompt_ast>');
  });

  it('formats plain map', () => {
    const value = { a: 1, b: 'two' };
    expect(formatValue(value, 'inline-hint')).toBe('{a: 1, b: "two"}');
  });
});
