import he from 'he';
import { describe, expect, it } from 'vitest';
import {
  extractStringValues,
  getHighlightChunks,
  getHighlightedParts,
} from '../highlight-utils';

describe('extractStringValues', () => {
  it('returns empty array for undefined or non-array', () => {
    expect(extractStringValues(undefined as any)).toEqual([]);
    expect(extractStringValues(null as any)).toEqual([]);
    expect(extractStringValues({} as any)).toEqual([]);
  });

  it('extracts plain string values', () => {
    const inputs = [{ value: 'hello world' }];
    expect(extractStringValues(inputs as any)).toEqual(['hello world']);
  });

  it('extracts values from JSON string', () => {
    const inputs = [{ value: '{"a":"foo","b":"bar"}' }];
    expect(extractStringValues(inputs as any)).toEqual(['foo', 'bar']);
  });

  it('splits repeated text by 2+ spaces', () => {
    const inputs = [{ value: 'foo  bar   baz' }];
    expect(extractStringValues(inputs as any)).toEqual(['foo', 'bar', 'baz']);
  });

  it('extracts string values from object input', () => {
    const inputs = [{ value: { a: 'foo', b: 2, c: 'bar' } }];
    expect(extractStringValues(inputs as any)).toEqual(['foo', 'bar']);
  });

  it('trims trailing whitespace and newlines from plain strings', () => {
    const inputs = [{ value: 'hello world  \n  ' }];
    expect(extractStringValues(inputs as any)).toEqual(['hello world']);
  });

  it('trims trailing whitespace from JSON string values', () => {
    const inputs = [{ value: '{"a":"foo  \\n  ","b":"bar\\t"}' }];
    expect(extractStringValues(inputs as any)).toEqual(['foo', 'bar']);
  });

  it('trims trailing whitespace from object values', () => {
    const inputs = [{ value: { a: 'foo  \n', b: 2, c: '  bar\t  ' } }];
    expect(extractStringValues(inputs as any)).toEqual(['foo', 'bar']);
  });

  it('handles strings with only whitespace by filtering them out', () => {
    const inputs = [{ value: { a: '   \n\t   ', b: 'valid', c: '' } }];
    expect(extractStringValues(inputs as any)).toEqual(['valid']);
  });
});

describe('getHighlightChunks', () => {
  const text = 'The quick brown fox jumps over the lazy dog.';

  it('returns only chunks present in text', () => {
    const chunks = ['quick', 'cat', 'dog'];
    expect(getHighlightChunks(text, chunks)).toEqual(['quick', 'dog']);
  });

  it('handles empty or missing chunks/text', () => {
    expect(getHighlightChunks('', ['foo'])).toEqual([]);
    expect(getHighlightChunks(text, [])).toEqual([]);
    expect(getHighlightChunks(text, [''])).toEqual([]);
  });

  it('escapes regex special characters in chunks', () => {
    const special = 'quick.';
    expect(getHighlightChunks(text, [special])).toEqual([]);
    expect(getHighlightChunks(`${text} quick.`, [special])).toEqual(['quick.']);
  });

  it('handles unicode and emoji', () => {
    const t = 'Hello 👋 world';
    expect(getHighlightChunks(t, ['👋'])).toEqual(['👋']);
  });
});

describe('getHighlightedParts', () => {
  it('returns non-highlighted text when no chunks provided', () => {
    const text = 'hello world';
    expect(getHighlightedParts(text, [])).toEqual([
      { text: 'hello world', highlight: false },
    ]);
  });

  it('highlights exact matches without extra whitespace', () => {
    const text = 'hello world test';
    const chunks = ['hello'];
    const result = getHighlightedParts(text, chunks);
    expect(result).toEqual([
      { text: 'hello', highlight: true },
      { text: ' world test', highlight: false },
    ]);
  });

  it('does not highlight extra whitespace around chunks', () => {
    const text = 'hello   world   test';
    const chunks = ['world'];
    const result = getHighlightedParts(text, chunks);
    expect(result).toEqual([
      { text: 'hello   ', highlight: false },
      { text: 'world', highlight: true },
      { text: '   test', highlight: false },
    ]);
  });

  it('highlights chunks with spaces exactly as they appear', () => {
    const text = 'hello world test world';
    const chunks = ['hello world'];
    const result = getHighlightedParts(text, chunks);
    expect(result).toEqual([
      { text: 'hello world', highlight: true },
      { text: ' test world', highlight: false },
    ]);
  });

  it('does not match chunks with different whitespace', () => {
    const text = 'hello  world test'; // two spaces
    const chunks = ['hello world']; // one space
    const result = getHighlightedParts(text, chunks);
    expect(result).toEqual([{ text: 'hello  world test', highlight: false }]);
  });

  it('handles multiple chunks with proper boundaries', () => {
    const text = 'the quick brown fox';
    const chunks = ['quick', 'fox'];
    const result = getHighlightedParts(text, chunks);
    expect(result).toEqual([
      { text: 'the ', highlight: false },
      { text: 'quick', highlight: true },
      { text: ' brown ', highlight: false },
      { text: 'fox', highlight: true },
    ]);
  });

  it('prioritizes longer chunks over shorter ones', () => {
    const text = 'hello world';
    const chunks = ['hello', 'hello world'];
    const result = getHighlightedParts(text, chunks);
    expect(result).toEqual([{ text: 'hello world', highlight: true }]);
  });

  it('handles overlapping chunks correctly', () => {
    const text = 'abcdef';
    const chunks = ['abc', 'cde'];
    const result = getHighlightedParts(text, chunks);
    // Should highlight 'abc' first (longer chunks are prioritized equally, so order matters)
    expect(result).toEqual([
      { text: 'abc', highlight: true },
      { text: 'def', highlight: false },
    ]);
  });

  it('handles real-world Question/Answer format with proper highlighting', () => {
    const text = `Question:       What is the meaning of life, and how can we find purpose in our existence? This question has puzzled philosophers, theologians, and scientists for centuries, and yet, it remains one of the most profound and elusive questions of our time.

As we navigate the complexities of our lives, we often find ourselves searching for answers to this question. We may turn to religion, spirituality, or personal relationships to find meaning, but the answer remains elusive. Can you provide some insight into this question, and help us understand the human experience?
Answer: `;

    const chunks = [
      'What is the meaning of life, and how can we find purpose in our existence? This question has puzzled philosophers, theologians, and scientists for centuries, and yet, it remains one of the most profound and elusive questions of our time.',
      'As we navigate the complexities of our lives, we often find ourselves searching for answers to this question. We may turn to religion, spirituality, or personal relationships to find meaning, but the answer remains elusive. Can you provide some insight into this question, and help us understand the human experience?',
    ];

    const result = getHighlightedParts(text, chunks);

    // Now that the function is fixed, we should get 5 parts as expected
    expect(result).toHaveLength(5);

    // First part: "Question:       " prefix (not highlighted)
    expect(result[0]).toBeDefined();
    expect(result[0]?.highlight).toBe(false);
    expect(result[0]?.text).toBe('Question:       ');

    // Second part: First chunk highlighted
    expect(result[1]).toBeDefined();
    expect(result[1]?.highlight).toBe(true);
    expect(result[1]?.text).toBe(chunks[0]);

    // Third part: Middle spacing (not highlighted)
    expect(result[2]).toBeDefined();
    expect(result[2]?.highlight).toBe(false);
    expect(result[2]?.text).toBe('\n\n');

    // Fourth part: Second chunk highlighted (including the ?)
    expect(result[3]).toBeDefined();
    expect(result[3]?.highlight).toBe(true);
    expect(result[3]?.text).toBe(chunks[1]);

    // Fifth part: Final "\nAnswer: " (not highlighted)
    expect(result[4]).toBeDefined();
    expect(result[4]?.highlight).toBe(false);
    expect(result[4]?.text).toBe('\nAnswer: ');
  });

  it('handles HTML entities and special characters without double encoding', () => {
    const text = "Here's a simple recipe for beef stew:";

    // This simulates what extractStringValues would return
    const chunks = ["Here's a simple recipe for beef stew:"];

    const result = getHighlightedParts(text, chunks);

    // Should get 1 part that's fully highlighted
    expect(result).toHaveLength(1);
    expect(result[0]).toBeDefined();
    expect(result[0]?.highlight).toBe(true);
    // The highlighted text should contain the original apostrophe, not HTML encoded
    expect(result[0]?.text).toBe("Here's a simple recipe for beef stew:");
    expect(result[0]?.text).not.toContain('&#x27;');
    expect(result[0]?.text).not.toContain('&apos;');
  });

  it('reproduces the HTML encoding mismatch issue in full pipeline', () => {
    // This test originally demonstrated the HTML encoding issue, but it's now fixed
    // Keeping it to show that the original problem no longer exists
    const originalText = "Here's a simple recipe for beef stew:";

    // Before the fix: text was HTML encoded in render-part.tsx
    const htmlEncodedText = he.encode(originalText);

    // Before the fix: extractStringValues also HTML encoded the chunks
    // After the fix: extractStringValues returns raw strings
    const mockInputs = [
      {
        name: 'content',
        value: originalText,
        toJSON: () => ({}),
        free: () => {},
        error: undefined,
      } as any,
    ];

    const extractedChunks = extractStringValues(mockInputs);

    // After the fix: chunks are no longer HTML encoded
    expect(htmlEncodedText).toBe('Here&#x27;s a simple recipe for beef stew:');
    expect(extractedChunks).toHaveLength(1);
    expect(extractedChunks[0]).toBe("Here's a simple recipe for beef stew:"); // Now raw

    // Since render-part.tsx was also fixed to not HTML encode text,
    // both text and chunks are now raw and match properly
    const result = getHighlightedParts(originalText, extractedChunks); // Use raw text

    expect(result).toHaveLength(1);
    expect(result[0]).toBeDefined();
    expect(result[0]?.highlight).toBe(true); // Now highlights properly
    expect(result[0]?.text).toBe("Here's a simple recipe for beef stew:"); // User sees raw text
    expect(result[0]?.text).not.toContain('&#x27;'); // No HTML encoding visible to user
  });

  it('demonstrates the correct behavior after fixing HTML encoding', () => {
    // After the fix: both text and chunks are raw strings and match properly
    const originalText = "Here's a simple recipe for beef stew:";

    // Text is no longer HTML encoded (fixed in render-part.tsx)
    const text = originalText; // Raw text

    // Chunks are also no longer HTML encoded (fixed in extractStringValues)
    const mockInputs = [
      {
        name: 'content',
        value: originalText,
        toJSON: () => ({}),
        free: () => {},
        error: undefined,
      } as any,
    ];

    const extractedChunks = extractStringValues(mockInputs);

    // Both text and chunks are raw strings - they match!
    expect(text).toBe("Here's a simple recipe for beef stew:");
    expect(extractedChunks[0]).toBe("Here's a simple recipe for beef stew:");

    // Highlighting works because they match
    const result = getHighlightedParts(text, extractedChunks);

    expect(result).toHaveLength(1);
    expect(result[0]).toBeDefined();
    expect(result[0]?.highlight).toBe(true); // Highlighting works!
    expect(result[0]?.text).toBe("Here's a simple recipe for beef stew:"); // Raw text shown to user
    expect(result[0]?.text).not.toContain('&#x27;'); // User sees correct characters
  });
});
