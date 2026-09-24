// The one piece of markdown the quiz's prose uses.
//
// Claims and compiler messages both name types in backticks -- "`int` is a
// strict subtype of `int | string`" -- because that is how the spec they are
// quoting writes them. Rendering the backticks literally leaves the reader to
// do the typesetting, so they become code spans.
//
// Nothing generates any other markdown, so nothing else is parsed: a claim is
// prose from BAML, not a document, and a parser for emphasis and links would
// be answering a question no one asked.

/** A run of text, either prose or the contents of a code span. */
export interface Segment {
  text: string;
  code: boolean;
}

/**
 * `text` split into prose and code spans.
 *
 * An unclosed span is prose: a stray backtick is likelier than a missing one,
 * and the reader is better served by seeing the text than by everything after
 * it turning into code.
 */
export function segments(text: string): Segment[] {
  const pieces = text.split('`');
  const closed = pieces.length % 2 === 1;
  return pieces
    .map((piece, at) => ({
      code: at % 2 === 1 && (closed || at < pieces.length - 1),
      text: piece,
    }))
    .filter((segment) => segment.text.length > 0);
}
