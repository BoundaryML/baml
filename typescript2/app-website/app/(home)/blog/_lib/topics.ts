import type { Post } from './get-posts';

// How the blog index sorts and filters posts. Every article is engineering: the blog is the
// engineering blog, so it has one topic, and the index opens on it. Releases are the only other view.

export const TOPICS = [{ key: 'engineering', label: 'Engineering' }] as const;

export type Topic = (typeof TOPICS)[number]['key'];

/** Whether `key` names one of the index's topics. */
export function isTopic(key: string): key is Topic {
  return TOPICS.some((t) => t.key === key);
}

/** A post's topics: engineering for every article, none for a release. */
export function topicsOf(post: Post): Topic[] {
  return post.type === 'release' ? [] : ['engineering'];
}

/** The label a post shows: Engineering, or Release for a release. */
export function topicLabel(post: Post) {
  return post.type === 'release' ? 'Release' : 'Engineering';
}

/** Whether a post shows on the index, in the feed, and in "Read next". Archived posts are outdated
 *  ones kept at their URLs for old links and search results, but not listed. */
export function isListed(post: Post) {
  return Boolean(post.isPublished) && !post.archived;
}

/** Whether a post shows on the blog index: every listed post, old or new. */
export function isOnIndex(post: Post) {
  return isListed(post);
}

/** Launch Week is six short posts in one week of January 2025, listed as one group. */
export const LAUNCH_WEEK_PREFIX = 'launch-week-day-';
