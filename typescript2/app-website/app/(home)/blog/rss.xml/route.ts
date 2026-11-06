import { SITE_URL } from '@/app/_lib/metadata';
import { isoPostDate } from '../_lib/format';
import { getPosts } from '../_lib/get-posts';
import { isListed, topicLabel } from '../_lib/topics';

// The blog's RSS 2.0 feed at /blog/rss.xml: every listed article and release, newest first. Archived
// posts are left out, as they are on the index. Built at deploy time, like the posts themselves.

export const dynamic = 'force-static';

/** `text` with the five XML special characters escaped. */
const escapeXml = (text: string) =>
  text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');

/** A post date as RFC 822, the form RSS readers expect. */
const rssDate = (date: string) =>
  new Date(`${isoPostDate(date)}T00:00:00Z`).toUTCString();

/** The feed, as `application/rss+xml`. */
export async function GET() {
  const posts = (await getPosts()).filter(isListed);
  const items = posts
    .map((post) => {
      const url = `${SITE_URL}/blog/${post.slug}`;
      const category = topicLabel(post);
      return [
        '    <item>',
        `      <title>${escapeXml(post.title)}</title>`,
        `      <link>${url}</link>`,
        `      <guid isPermaLink="true">${url}</guid>`,
        `      <pubDate>${rssDate(post.date)}</pubDate>`,
        `      <description>${escapeXml(post.description ?? '')}</description>`,
        category ? `      <category>${escapeXml(category)}</category>` : '',
        post.author
          ? `      <dc:creator>${escapeXml(post.author.name)}</dc:creator>`
          : '',
        '    </item>',
      ]
        .filter(Boolean)
        .join('\n');
    })
    .join('\n');

  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <channel>
    <title>Boundary blog</title>
    <link>${SITE_URL}/blog</link>
    <atom:link href="${SITE_URL}/blog/rss.xml" rel="self" type="application/rss+xml"/>
    <description>Notes from the team building BAML and Boundary: tutorials, research and release notes.</description>
    <language>en-us</language>
${items}
  </channel>
</rss>
`;

  return new Response(xml, {
    headers: { 'Content-Type': 'application/rss+xml; charset=utf-8' },
  });
}
