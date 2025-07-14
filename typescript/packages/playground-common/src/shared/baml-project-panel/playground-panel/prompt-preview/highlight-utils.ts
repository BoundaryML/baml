import type {
  WasmChatMessagePart,
  WasmParam,
} from '@gloo-ai/baml-schema-wasm-web';
import he from 'he';

/**
 * Cleans a string by trimming all types of whitespace including newlines and tabs
 */
const cleanWhitespace = (str: string): string => {
  return str.replace(/^\s+|\s+$/g, '');
};

/**
 * Extracts string values from WasmParam inputs, handling JSON and plain strings.
 * Returns raw strings without HTML encoding for proper highlighting matching.
 */
export const extractStringValues = (inputs: WasmParam[]): string[] => {
  if (!inputs || !Array.isArray(inputs)) return [];

  return inputs.flatMap((input) => {
    if (typeof input.value === 'string') {
      try {
        // Try to parse the string as JSON
        const parsed = JSON.parse(input.value);
        if (typeof parsed === 'object') {
          const result = Object.values(parsed)
            .filter((val): val is string => typeof val === 'string')
            .map((val) => cleanWhitespace(val)) // Clean whitespace from JSON values
            .filter(Boolean); // Remove empty strings after cleaning
          return result;
        }
        // Split the string into individual phrases if it contains repeated text
        const phrases = parsed.split(/\s{2,}/).filter(Boolean);
        const result = phrases
          .map((phrase: string) => cleanWhitespace(phrase)) // Clean before returning
          .filter(Boolean); // Remove empty strings after cleaning
        return result;
      } catch {
        // If parsing fails, treat it as a regular string
        // Split the string into individual phrases if it contains repeated text
        const phrases = input.value.split(/\s{2,}/).filter(Boolean);
        const result = phrases
          .map((phrase: string) => cleanWhitespace(phrase)) // Clean before returning
          .filter(Boolean); // Remove empty strings after cleaning
        return result;
      }
    }
    if (typeof input.value === 'object') {
      const result = Object.values(input.value)
        .filter((val): val is string => typeof val === 'string')
        .map((val) => cleanWhitespace(val)) // Clean whitespace from object values
        .filter(Boolean); // Remove empty strings after cleaning
      return result;
    }
    return [];
  });
};

/**
 * Returns the list of highlight chunks that appear in the text.
 * Escapes regex characters and checks for matches.
 */
export const getHighlightChunks = (
  text: string,
  allChunks: string[],
): string[] => {
  return allChunks.filter((chunk) => {
    if (!chunk || !text) return false;
    try {
      // Escape special regex characters in the chunk
      const escapedChunk = chunk.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      // Use unicode flag to handle emojis correctly
      const regex = new RegExp(escapedChunk, 'gu');
      const matches = text.match(regex);
      // Include chunks that appear at least once in the text
      return matches && matches.length > 0;
    } catch (e) {
      console.error('Error matching chunk', e);
      return false;
    }
  });
};

/**
 * Returns the first non-empty line from the first text part in the array.
 */
export function getFirstLine(parts: WasmChatMessagePart[]): string {
  for (const part of parts) {
    if (part.is_text?.()) {
      const text = part.as_text();
      if (text) {
        const decodedText = he.decode(text);
        const lines = decodedText.split('\n');
        if (lines.length > 0 && lines[0]?.trim()) {
          return lines[0]?.trim() ?? '';
        }
      }
    }
  }
  return '';
}

/**
 * Splits text into parts, marking which parts should be highlighted based on highlightChunks.
 * Uses exact matching to avoid highlighting extra whitespace.
 */
export function getHighlightedParts(
  text: string,
  highlightChunks: string[],
): Array<{ text: string; highlight: boolean }> {
  if (!highlightChunks?.length) return [{ text, highlight: false }];

  // 1) Filter + sort
  const validChunks = highlightChunks
    .filter((c): c is string => !!c)
    .sort((a, b) => b.length - a.length);

  if (!validChunks.length) return [{ text, highlight: false }];

  // 2) Escape every chunk for use in a RegExp
  const escapeRe = (s: string) => s.replace(/[-\/\\^$*+?.()|[\]{}]/g, '\\$&');

  const pattern = validChunks.map(escapeRe).join('|');

  // 3) Build a global, capturing‐group regex (no more /m or /s needed)
  const regex = new RegExp(`(${pattern})`, 'g');

  // 4) Split on the chunks *and* keep them in the result
  const tokens = text.split(regex);

  // 5) Map into your parts, marking only the exact chunks as highlighted
  return tokens
    .filter((t) => t.length > 0) // (optional: drop any empty‐string tokens)
    .map((t) => ({
      text: t,
      highlight: validChunks.includes(t),
    }));
}
