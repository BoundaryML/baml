/**
 * BEP Import Utilities
 *
 * Functions for importing markdown files and stripping embedded comments/frontmatter.
 * Used for the round-trip workflow: export with comments → edit externally → import clean.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Frontmatter Stripping
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Strip YAML frontmatter from markdown content.
 * Frontmatter is delimited by --- at the start of the file.
 */
export function stripFrontmatter(markdown: string): string {
  // Match frontmatter at the very beginning: ---\n...\n---\n
  const frontmatterPattern = /^---\n[\s\S]*?\n---\n/;
  return markdown.replace(frontmatterPattern, "");
}

// ─────────────────────────────────────────────────────────────────────────────
// Comment Stripping
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Strip all embedded comments from markdown content.
 * Handles:
 * - block-comments section (<!-- block-comments -->...<!-- /block-comments -->)
 * - comments section (<!-- comments -->...<!-- /comments -->)
 * - Individual @type comments (<!-- @type ... -->...<!-- /@type -->)
 *
 * @param markdown - The markdown content with embedded comments
 * @returns Clean markdown with all comments removed
 */
export function stripCommentsFromMarkdown(markdown: string): string {
  let result = markdown;

  // 1. Strip block-comments section
  // Format: \n---\n\n<!-- block-comments -->\n...\n<!-- /block-comments -->
  const blockCommentsSectionPattern =
    /\n---\n\n<!-- block-comments -->[\s\S]*?<!-- \/block-comments -->\s*\n?/g;
  result = result.replace(blockCommentsSectionPattern, "\n");

  // Without --- separator
  const blockCommentsSectionNoSeparator =
    /\n<!-- block-comments -->[\s\S]*?<!-- \/block-comments -->\s*\n?/g;
  result = result.replace(blockCommentsSectionNoSeparator, "\n");

  // 2. Strip comments section (general comments)
  // Format: \n---\n\n<!-- comments -->\n...\n<!-- /comments -->
  const commentsSectionPattern =
    /\n---\n\n<!-- comments -->[\s\S]*?<!-- \/comments -->\s*\n?/g;
  result = result.replace(commentsSectionPattern, "\n");

  // Without --- separator
  const commentsSectionNoSeparator =
    /\n<!-- comments -->[\s\S]*?<!-- \/comments -->\s*\n?/g;
  result = result.replace(commentsSectionNoSeparator, "\n");

  // 3. Strip individual @type comment blocks (discussion, concern, question, etc.)
  // Format: <!-- @type by Author | "context" -->\n> content\n<!-- /@type -->
  const typeCommentPattern =
    /\n?\s*<!-- @\w+[^>]*-->[\s\S]*?<!-- \/@\w+ -->\s*\n?/g;
  result = result.replace(typeCommentPattern, "\n");

  // 4. Clean up multiple consecutive empty lines (max 2)
  result = result.replace(/\n{3,}/g, "\n\n");

  // 5. Trim trailing whitespace but keep final newline
  result = result.trimEnd() + "\n";

  return result;
}

/**
 * Strip both frontmatter and comments from markdown.
 */
export function cleanImportedMarkdown(markdown: string): string {
  return stripCommentsFromMarkdown(stripFrontmatter(markdown));
}

// ─────────────────────────────────────────────────────────────────────────────
// File Parsing
// ─────────────────────────────────────────────────────────────────────────────

export interface ParsedReadme {
  content: string;
  extractedTitle?: string;
}

export interface ParsedPage {
  slug: string;
  title: string;
  content: string;
}

/**
 * Parse an imported README.md file.
 * Strips frontmatter, comments, and extracts the title from the first heading.
 *
 * @param markdown - The raw markdown content
 * @returns Parsed content with optional extracted title
 */
export function parseImportedReadme(markdown: string): ParsedReadme {
  const cleanContent = cleanImportedMarkdown(markdown);

  // Extract title from first # heading
  const titleMatch = cleanContent.match(/^#\s+(.+)$/m);
  const extractedTitle = titleMatch ? titleMatch[1].trim() : undefined;

  return {
    content: cleanContent,
    extractedTitle,
  };
}

/**
 * Parse an imported page file.
 * Strips frontmatter, comments, and extracts slug from filename and title from heading.
 *
 * @param markdown - The raw markdown content
 * @param filename - The filename (e.g., "background.md")
 * @returns Parsed page with slug, title, and clean content
 */
export function parseImportedPage(
  markdown: string,
  filename: string
): ParsedPage {
  const cleanContent = cleanImportedMarkdown(markdown);

  // Extract slug from filename (remove .md extension)
  const slug = filename.replace(/\.md$/i, "").toLowerCase();

  // Extract title from first # heading, or use slug as fallback
  const titleMatch = cleanContent.match(/^#\s+(.+)$/m);
  const title = titleMatch
    ? titleMatch[1].trim()
    : slug.charAt(0).toUpperCase() + slug.slice(1);

  return {
    slug,
    title,
    content: cleanContent,
  };
}

// ─────────────────────────────────────────────────────────────────────────────
// Validation
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Validate that a slug is URL-safe.
 * Allowed: lowercase alphanumeric characters and hyphens.
 */
export function isValidSlug(slug: string): boolean {
  return /^[a-z0-9]+(-[a-z0-9]+)*$/.test(slug);
}

/**
 * Sanitize a filename to a valid slug.
 * Converts to lowercase, replaces spaces/underscores with hyphens,
 * removes invalid characters.
 */
export function sanitizeSlug(filename: string): string {
  return filename
    .replace(/\.md$/i, "") // Remove .md extension
    .toLowerCase()
    .replace(/[\s_]+/g, "-") // Replace spaces and underscores with hyphens
    .replace(/[^a-z0-9-]/g, "") // Remove invalid characters
    .replace(/-+/g, "-") // Collapse multiple hyphens
    .replace(/^-|-$/g, ""); // Remove leading/trailing hyphens
}

/**
 * Validate that content is not empty after stripping comments.
 */
export function hasContent(content: string): boolean {
  // Strip all whitespace and check if anything remains
  return content.replace(/\s/g, "").length > 0;
}
