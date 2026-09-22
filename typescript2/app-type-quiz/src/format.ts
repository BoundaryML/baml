// The real BAML formatter, compiled to WebAssembly on its own.
//
// Cases are generated as text, and a learner should be reading them laid out
// the way this project lays out every other program: the question is what the
// code means, not how it is spaced. Running the formatter rather than
// generating already-formatted text is what keeps the generator from having
// to follow the formatter's rules as they change.

import init, { format } from '@quiz/fmt';

let started: Promise<unknown> | undefined;

/** Instantiate the formatter. Safe to call more than once. */
export function loadFormatter(): Promise<unknown> {
  const already = started;
  if (already !== undefined) {
    return already;
  }
  const starting = init();
  started = starting;
  return starting;
}

/**
 * `source`, laid out as `baml fmt` lays it out.
 *
 * Source that does not parse comes back unchanged from the formatter itself.
 * A throw means source that *did* parse could not be laid out, which is a
 * defect in the formatter; the learner is better served by the unformatted
 * program than by an empty screen, so it is reported and the original stands.
 */
export function formatSource(source: string): string {
  try {
    return format(source);
  } catch (error) {
    console.error('the formatter failed on a generated case', error);
    return source;
  }
}
