/**
 * The program sources that the fixtures refer to, by the file name that a
 * `position` event carries, and the lookup of a line by its text.
 */

import { QUOTES_BAML, QUOTES_BAML_FILE } from "./quotes.baml";
import { TRIP_BAML, TRIP_BAML_FILE } from "./trip.baml";

export const FIXTURE_SOURCES: Readonly<Record<string, string>> = {
  [TRIP_BAML_FILE]: TRIP_BAML,
  [QUOTES_BAML_FILE]: QUOTES_BAML,
};

/**
 * The 1-based line of the first occurrence of `needle` inside the body of the
 * function `fn` of `file`. Fixtures use this instead of literal line numbers,
 * so an edit of the program text cannot leave them stale.
 */
export function lineIn(file: string, fn: string, needle: string): number {
  const text = FIXTURE_SOURCES[file];
  if (text === undefined) throw new Error(`no fixture program ${file}`);
  const lines = text.split("\n");
  const start = lines.findIndex((line) => line.startsWith(`function ${fn}(`));
  if (start === -1) throw new Error(`${file} has no function ${fn}`);
  // The search starts after the signature line, which can contain the needle.
  for (let i = start + 1; i < lines.length; i++) {
    const line = lines[i] as string;
    if (line.startsWith("}")) break;
    if (line.includes(needle)) return i + 1;
  }
  throw new Error(`function ${fn} of ${file} has no line that contains ${needle}`);
}
