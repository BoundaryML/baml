/**
 * Shared heading extraction for BEP content.
 *
 * Both the rendered markdown (BepContent) and the table of contents derive
 * heading anchor ids from this module. Keeping one implementation is what
 * guarantees TOC clicks land on an element that actually exists.
 */

export interface HeadingInfo {
  /** Anchor id (deduped slug) */
  id: string;
  text: string;
  level: number;
  /** 1-indexed line number within the frontmatter-stripped content */
  line: number;
}

export function stripFrontmatter(content: string): string {
  return content.replace(/^---[\s\S]*?---\n*/, "");
}

export function slugifyHeading(value: string): string {
  return value
    .toLowerCase()
    .trim()
    .replace(/[^\w\s-]/g, "")
    .replace(/[\s_]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
}

/**
 * Extract headings (h1-h4) from markdown, skipping fenced code blocks so
 * `# comment` lines inside ``` fences don't produce phantom TOC entries or
 * shift the duplicate-slug counters.
 *
 * @param content - Markdown WITHOUT frontmatter (see stripFrontmatter)
 */
export function extractHeadings(content: string): HeadingInfo[] {
  if (!content) return [];

  const headings: HeadingInfo[] = [];
  const slugCounts = new Map<string, number>();
  const lines = content.split("\n");
  let fenceMarker: string | null = null;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fenceMatch = /^\s*(```+|~~~+)/.exec(line);
    if (fenceMatch) {
      if (fenceMarker === null) {
        fenceMarker = fenceMatch[1][0].repeat(3);
      } else if (fenceMatch[1].startsWith(fenceMarker)) {
        fenceMarker = null;
      }
      continue;
    }
    if (fenceMarker !== null) continue;

    const match = /^(#{1,4})\s+(.+)$/.exec(line);
    if (!match) continue;

    const level = match[1].length;
    const text = match[2].trim();
    const baseSlug = slugifyHeading(text);
    if (!baseSlug) continue;

    const count = (slugCounts.get(baseSlug) ?? 0) + 1;
    slugCounts.set(baseSlug, count);
    const id = count === 1 ? baseSlug : `${baseSlug}-${count}`;

    headings.push({ id, text, level, line: i + 1 });
  }

  return headings;
}
