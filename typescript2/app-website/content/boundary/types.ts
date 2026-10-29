// Content is plain typed TypeScript so the site stays dependency-free: no MDX, no markdown parser,
// no CMS. Add a post or a case study by adding an object to the array in content/blog.ts or
// content/case-studies.ts. See "Adding a blog post or case study" in the README.

/**
 * A body is an array of blocks. `paragraph`, `list` and `quote` text may contain two inline forms:
 *   `code`                 -> a mono chip
 *   [label](https://…)     -> a link in the site's underline style
 * Everything else is literal, so you never have to escape anything.
 */
export type Block =
  | { type: 'paragraph'; text: string }
  | { type: 'heading'; level: 2 | 3; id: string; text: string }
  | { type: 'code'; lang: string; filename?: string; code: string }
  | { type: 'list'; ordered?: boolean; items: string[] }
  | { type: 'quote'; text: string; cite?: string }
  | { type: 'image'; src: string; alt: string; caption?: string };

export type Post = {
  slug: string;
  title: string;
  dek: string;
  /** ISO date, YYYY-MM-DD. Posts are sorted newest first. */
  date: string;
  author: string;
  tags: string[];
  /** Marks sample copy so it's easy to find before launch (data only; nothing shows on the page). */
  sample?: boolean;
  body: Block[];
};

export type CaseStudy = {
  slug: string;
  /** Company name, shown on the painting panel. */
  company: string;
  /** One-line outcome headline. */
  title: string;
  dek: string;
  date: string;
  tags: string[];
  /** Marks a placeholder study so it's easy to find before launch (data only; nothing shows on the page). */
  placeholder?: boolean;
  /** One of the paintings in /public. Boundary's greens only; the dusk paintings belong to BAML. */
  painting: string;
  /** CSS object-position for the painting, used on both the card and the page hero. */
  crop: string;
  /** How far the index card zooms into `crop`. The card frame is nearly the painting's own aspect
   *  ratio, so object-position alone moves the image by a few pixels and every card comes out
   *  looking identical; the zoom is what makes three studies read as three views. The page hero is
   *  large enough to show the painting whole, so it ignores this. */
  cardZoom: number;
  /** Rendered as a definition list under the title. Keep it to four pairs so it fits one row. */
  facts: { term: string; value: string }[];
  sections: { id: string; heading: string; body: Block[] }[];
  /** Optional headline metrics. Leave out until the customer signs off on real numbers; the Results
   *  section (and its entry in "On this page") only renders when this has items. */
  results?: { value: string; label: string }[];
  /** Optional customer quote. Leave out until there is a real one; renders only when present. */
  quote?: { text: string; name: string; role: string; company: string };
};

const DATE_FMT = new Intl.DateTimeFormat('en-US', {
  day: 'numeric',
  month: 'short',
  timeZone: 'UTC',
  year: 'numeric',
});

/** "Oct 9, 2026" from "2026-10-09". Fixed locale and UTC, so the server and the client agree
 * and a date never slips a day for a reader west of Greenwich. */
export function formatDate(iso: string): string {
  return DATE_FMT.format(new Date(`${iso}T00:00:00Z`));
}

export const byNewest = <T extends { date: string }>(a: T, b: T) =>
  a.date < b.date ? 1 : a.date > b.date ? -1 : 0;
