// Each episode's YouTube views and length. Nothing here calls an API when the page loads:
// - youtube-views.json is a snapshot of every video's views. Refresh it whenever you like with
//   `node --experimental-strip-types scripts/refresh-podcast-views.ts`, and commit the result.
// - The season playlist's public RSS feed (no key) carries live views for its newest 15 videos.
//   Next caches it for an hour, so at most one reader an hour waits on it.
// A count is the larger of the two, so it only lags, it never drops. The feed also gives the video
// for an episode the ai-that-works feed hasn't linked yet.

import snapshot from './youtube-views.json';

const SNAPSHOT: Record<string, number> = snapshot.views;
const LENGTHS: Record<string, number> = snapshot.seconds;

/** The current season's playlist. */
const PLAYLIST = 'PLi60mUelRAbFqfgymVfZttlkIyt0XHZjt';

export type FeedVideo = { id: string; published: string; views: number };

/** The playlist's newest videos with their views; none if YouTube doesn't answer. */
export async function fetchPlaylistVideos(): Promise<FeedVideo[]> {
  try {
    const res = await fetch(
      `https://www.youtube.com/feeds/videos.xml?playlist_id=${PLAYLIST}`,
      { next: { revalidate: 3600 }, signal: AbortSignal.timeout(5000) },
    );
    if (!res.ok) return [];
    const xml = await res.text();
    return [...xml.matchAll(/<entry>([\s\S]*?)<\/entry>/g)].flatMap(([, e]) => {
      const id = e.match(/<yt:videoId>([^<]+)</)?.[1];
      const published = e.match(/<published>([^<]+)</)?.[1];
      const views = Number(e.match(/views="(\d+)"/)?.[1] ?? 0);
      return id && published ? [{ id, published, views }] : [];
    });
  } catch {
    return [];
  }
}

/** A video's views: the larger of the feed's and the snapshot's. */
export function viewsFor(id: string, feed: FeedVideo[]): number | undefined {
  const counts = [feed.find((v) => v.id === id)?.views, SNAPSHOT[id]].filter(
    (n): n is number => n !== undefined,
  );
  return counts.length > 0 ? Math.max(...counts) : undefined;
}

/** A video's length in seconds, if the snapshot has it. */
export function lengthFor(id: string): number | undefined {
  return LENGTHS[id];
}

/** A length the way YouTube badges it: 58:12, 1:02:15. */
export function formatLength(total: number): string {
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`;
}

/** A view count the way YouTube writes it: 468, 2.3K, 73K. */
export function formatViews(n: number): string {
  if (n < 1000) return String(n);
  if (n < 10_000) return `${(n / 1000).toFixed(1).replace(/\.0$/, '')}K`;
  if (n < 1_000_000) return `${Math.round(n / 1000)}K`;
  return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, '')}M`;
}
