// Server-side Notion reader for the issue detail page. The pipeline pushes
// issues to Notion boards one-way (see services/notion_fixer); this pulls the
// human-facing side back: the page's actual content blocks and its comment
// thread. Degrades to link-only when NOTION_TOKEN is absent.

const NOTION_TOKEN = process.env.NOTION_TOKEN ?? '';
const NOTION_VERSION = '2022-06-28';
const API = 'https://api.notion.com/v1';

/** A Notion comment with its author resolved to a display name. */
export type NotionComment = {
  author: string;
  text: string;
  createdAt: string;
};

/** The pulled Notion side of an issue: page URL, content as markdown, comments. */
export type NotionContent = {
  url: string;
  contentMd: string | null; // null when token absent or fetch fails
  comments: NotionComment[];
};

/**
 * Builds the public Notion URL for a page id (no token required).
 * @param pageId - the Notion page id, with or without dashes
 * @returns the notion.so URL for the page
 */
export function notionPageUrl(pageId: string): string {
  return `https://notion.so/${pageId.replace(/-/g, '')}`;
}

/**
 * Authenticated GET against the Notion API.
 * @param path - path + query under the API root
 * @returns the parsed JSON body, or null on any failure
 */
async function notionGet<T>(path: string): Promise<T | null> {
  try {
    const r = await fetch(`${API}${path}`, {
      headers: {
        Authorization: `Bearer ${NOTION_TOKEN}`,
        'Notion-Version': NOTION_VERSION,
      },
      cache: 'no-store',
    });
    if (!r.ok) return null;
    return (await r.json()) as T;
  } catch {
    return null;
  }
}

type RichText = Array<{
  plain_text?: string;
  href?: string | null;
  annotations?: { bold?: boolean; italic?: boolean; code?: boolean };
}>;

/**
 * Flattens Notion rich text into markdown (bold/italic/code/links).
 * @param rt - the rich_text array from a block or comment
 * @returns the markdown string
 */
function richToMd(rt: RichText | undefined): string {
  return (rt ?? [])
    .map((t) => {
      let s = t.plain_text ?? '';
      if (t.annotations?.code) s = `\`${s}\``;
      if (t.annotations?.bold) s = `**${s}**`;
      if (t.annotations?.italic) s = `*${s}*`;
      if (t.href) s = `[${s}](${t.href})`;
      return s;
    })
    .join('');
}

type Block = {
  type: string;
  [k: string]: any;
};

/**
 * Converts one Notion block to a markdown line (common block types only).
 * @param b - the block object
 * @returns the markdown line, or null for unsupported/empty blocks
 */
function blockToMd(b: Block): string | null {
  const d = b[b.type] ?? {};
  const text = richToMd(d.rich_text);
  switch (b.type) {
    case 'heading_1':
      return `# ${text}`;
    case 'heading_2':
      return `## ${text}`;
    case 'heading_3':
      return `### ${text}`;
    case 'paragraph':
      return text || null;
    case 'bulleted_list_item':
      return `- ${text}`;
    case 'numbered_list_item':
      return `1. ${text}`;
    case 'to_do':
      return `- [${d.checked ? 'x' : ' '}] ${text}`;
    case 'quote':
      return `> ${text}`;
    case 'code':
      return `\`\`\`${d.language ?? ''}\n${text}\n\`\`\``;
    case 'divider':
      return '---';
    case 'callout':
      return `> ${text}`;
    default:
      return null;
  }
}

/**
 * Pulls a Notion page's content blocks (as markdown) and comment thread.
 * Comment authors are resolved to user names (best-effort, one lookup per
 * distinct author).
 * @param pageId - the Notion page id recorded on the issue
 * @returns the page URL, content markdown, and comments; content/comments are
 *   empty when NOTION_TOKEN is unset or requests fail
 */
export async function getNotionContent(pageId: string): Promise<NotionContent> {
  const out: NotionContent = {
    url: notionPageUrl(pageId),
    contentMd: null,
    comments: [],
  };
  if (!NOTION_TOKEN) return out;

  // page content blocks (paginated)
  const lines: string[] = [];
  let cursor: string | null = null;
  do {
    const page: {
      results: Block[];
      next_cursor: string | null;
      has_more: boolean;
    } | null = await notionGet(
      `/blocks/${pageId}/children?page_size=100${cursor ? `&start_cursor=${cursor}` : ''}`,
    );
    if (!page) break;
    for (const b of page.results) {
      const md = blockToMd(b);
      if (md !== null) lines.push(md);
    }
    cursor = page.has_more ? page.next_cursor : null;
  } while (cursor);
  if (lines.length) out.contentMd = lines.join('\n\n');

  // comment thread (paginated) + author name resolution
  type RawComment = {
    rich_text: RichText;
    created_time: string;
    created_by?: { id?: string };
  };
  const raw: RawComment[] = [];
  cursor = null;
  do {
    const page: {
      results: RawComment[];
      next_cursor: string | null;
      has_more: boolean;
    } | null = await notionGet(
      `/comments?block_id=${pageId}&page_size=100${cursor ? `&start_cursor=${cursor}` : ''}`,
    );
    if (!page) break;
    raw.push(...page.results);
    cursor = page.has_more ? page.next_cursor : null;
  } while (cursor);

  const authorIds = [
    ...new Set(raw.map((c) => c.created_by?.id).filter(Boolean)),
  ] as string[];
  const names = new Map<string, string>(
    (
      await Promise.all(
        authorIds.map(async (uid) => {
          const u = await notionGet<{ name?: string }>(`/users/${uid}`);
          return [uid, u?.name ?? 'someone'] as const;
        }),
      )
    ).filter(([, n]) => n),
  );
  out.comments = raw.map((c) => ({
    author: names.get(c.created_by?.id ?? '') ?? 'someone',
    text: richToMd(c.rich_text),
    createdAt: c.created_time,
  }));
  return out;
}
