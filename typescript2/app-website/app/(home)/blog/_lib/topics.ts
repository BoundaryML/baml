import type { Post } from './get-posts';

// How the blog index sorts and filters posts. A post's topics come from its frontmatter tags, which
// were written by hand over the years in mixed case ("Engineering", "engineering"), so they're
// matched lowercased. Launch Week posts count as announcements. A post can carry more than one topic;
// its first one is its primary topic, used for "Read next".

export const TOPICS = [
  { key: 'research', label: 'Research' },
  { key: 'engineering', label: 'Engineering' },
  { key: 'tutorials', label: 'Tutorials' },
  { key: 'announcements', label: 'Announcements' },
] as const;

export type Topic = (typeof TOPICS)[number]['key'];

const ALIASES: Record<string, Topic> = { 'launch week': 'announcements' };

/** Whether `key` names one of the index's topics. */
export function isTopic(key: string): key is Topic {
  return TOPICS.some((t) => t.key === key);
}

/** A post's topics, in its tags' order, without repeats. */
export function topicsOf(post: Post): Topic[] {
  const out: Topic[] = [];
  for (const tag of post.tags ?? []) {
    const key = tag.trim().toLowerCase();
    const topic = ALIASES[key] ?? (isTopic(key) ? key : undefined);
    if (topic && !out.includes(topic)) out.push(topic);
  }
  return out;
}

/** The display name of a post's first topic, or of its type for a release. A post whose tags name
 *  no topic ("company") shows its first tag, capitalized. */
export function topicLabel(post: Post) {
  if (post.type === 'release') return 'Release';
  const first = topicsOf(post)[0];
  const label = TOPICS.find((t) => t.key === first)?.label;
  if (label) return label;
  const tag = post.tags?.[0]?.trim();
  return tag ? tag[0].toUpperCase() + tag.slice(1) : undefined;
}

/** Whether a post shows on the index, in the feed, and in "Read next". Archived posts are outdated
 *  ones kept at their URLs for old links and search results, but not listed. */
export function isListed(post: Post) {
  return Boolean(post.isPublished) && !post.archived;
}

/** How far back the blog index goes, in months. */
const INDEX_MONTHS = 2;

/** Whether a post shows on the blog index: a listed post from the last INDEX_MONTHS months. Older
 *  posts keep their URLs and stay in the feed and in "Read next"; only the index leaves them out. */
export function isOnIndex(post: Post, now = new Date()) {
  const cutoff = new Date(now);
  cutoff.setMonth(cutoff.getMonth() - INDEX_MONTHS);
  return isListed(post) && new Date(post.date) >= cutoff;
}

/** Launch Week is six short posts in one week of January 2025, listed as one group. */
export const LAUNCH_WEEK_PREFIX = 'launch-week-day-';
