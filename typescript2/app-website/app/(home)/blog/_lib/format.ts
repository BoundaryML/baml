// Post dates come from frontmatter in two shapes ("July 29, 2024" and "2026-10-25"). Both are shown
// the way the Boundary site shows dates ("Jul 29, 2024"), read in UTC so a date-only value never
// slips a day.
const DATE_FMT = new Intl.DateTimeFormat('en-US', {
  day: 'numeric',
  month: 'short',
  timeZone: 'UTC',
  year: 'numeric',
});

const parse = (date: string) =>
  new Date(/^\d{4}-\d{2}-\d{2}$/.test(date) ? `${date}T00:00:00Z` : date);

/** "Oct 25, 2026", for display. */
export function formatPostDate(date: string) {
  return DATE_FMT.format(parse(date));
}

/** "2026-10-25", for a `<time dateTime>`. */
export function isoPostDate(date: string) {
  return parse(date).toISOString().slice(0, 10);
}
