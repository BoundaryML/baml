#!/usr/bin/env -S node --experimental-strip-types

// Refreshes app/(home)/podcast/youtube-views.json: the views and length of every ai that works
// episode on YouTube. Run it whenever the numbers feel stale, then commit the file:
//
//   node --experimental-strip-types scripts/refresh-podcast-views.ts
//
// With YOUTUBE_API_KEY set it asks the YouTube Data API (two requests). Without one it reads each
// video's public watch page, one a second; YouTube rate-limits that, so when it starts refusing,
// the script stops and keeps what it has. A video it can't read keeps its old numbers, and a view
// count never goes down.

import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';

const FEED =
  'https://raw.githubusercontent.com/ai-that-works/ai-that-works/refs/heads/main/data.json';
const PLAYLIST_FEED =
  'https://www.youtube.com/feeds/videos.xml?playlist_id=PLi60mUelRAbFqfgymVfZttlkIyt0XHZjt';
const OUT = path.join(
  import.meta.dirname,
  '../app/(home)/podcast/youtube-views.json',
);

const VIDEO_ID = /(?:youtu\.be\/|watch\?v=|embed\/)([^&?#\s]+)/;

type Stats = { seconds?: number; views?: number };

async function videoIds(): Promise<string[]> {
  const feed = (await (await fetch(FEED)).json()) as {
    episodes: { links?: { youtube?: string }; media?: { url?: string } }[];
  };
  const ids = new Set<string>();
  for (const e of feed.episodes) {
    const id = (e.links?.youtube ?? e.media?.url ?? '').match(VIDEO_ID)?.[1];
    if (id) ids.add(id);
  }
  // The playlist has the newest videos, including ones the feed hasn't linked yet.
  const xml = await (await fetch(PLAYLIST_FEED)).text();
  for (const [, id] of xml.matchAll(/<yt:videoId>([^<]+)</g)) ids.add(id);
  return [...ids];
}

/** An ISO 8601 duration (PT1H2M3S) in seconds. */
function seconds(iso: string): number {
  const [, h, m, s] = iso.match(/PT(?:(\d+)H)?(?:(\d+)M)?(?:(\d+)S)?/) ?? [];
  return Number(h ?? 0) * 3600 + Number(m ?? 0) * 60 + Number(s ?? 0);
}

/** Every video's views and length from the YouTube Data API, 50 to a request. */
async function fromApi(
  ids: string[],
  key: string,
): Promise<Record<string, Stats>> {
  const out: Record<string, Stats> = {};
  for (let i = 0; i < ids.length; i += 50) {
    const url = new URL('https://www.googleapis.com/youtube/v3/videos');
    url.searchParams.set('part', 'statistics,contentDetails');
    url.searchParams.set('id', ids.slice(i, i + 50).join(','));
    url.searchParams.set('key', key);
    const res = await fetch(url);
    if (!res.ok)
      throw new Error(`YouTube API: ${res.status} ${await res.text()}`);
    const data = (await res.json()) as {
      items?: {
        contentDetails?: { duration?: string };
        id: string;
        statistics?: { viewCount?: string };
      }[];
    };
    for (const item of data.items ?? []) {
      const views = Number(item.statistics?.viewCount);
      const duration = item.contentDetails?.duration;
      out[item.id] = {
        seconds: duration ? seconds(duration) : undefined,
        views: Number.isFinite(views) ? views : undefined,
      };
    }
  }
  return out;
}

/** Each video's views and length from its watch page, one a second, until YouTube says slow down. */
async function fromPages(ids: string[]): Promise<Record<string, Stats>> {
  const out: Record<string, Stats> = {};
  for (const id of ids) {
    const res = await fetch(`https://www.youtube.com/watch?v=${id}`, {
      headers: { 'Accept-Language': 'en', 'User-Agent': 'Mozilla/5.0' },
      signal: AbortSignal.timeout(20_000),
    }).catch(() => undefined);
    if (res && (res.status === 429 || res.url.includes('/sorry/'))) {
      console.warn(
        `YouTube is rate-limiting this machine; stopping after ${Object.keys(out).length} videos. Try again later, or set YOUTUBE_API_KEY.`,
      );
      break;
    }
    const html = (await res?.text()) ?? '';
    const views = html.match(/"viewCount":"(\d+)"/)?.[1];
    const length = html.match(/"lengthSeconds":"(\d+)"/)?.[1];
    if (views || length)
      out[id] = {
        seconds: length ? Number(length) : undefined,
        views: views ? Number(views) : undefined,
      };
    await new Promise((r) => setTimeout(r, 1000));
  }
  return out;
}

const old = JSON.parse(await readFile(OUT, 'utf8')) as {
  seconds?: Record<string, number>;
  views: Record<string, number>;
};
const ids = await videoIds();
const key = process.env.YOUTUBE_API_KEY;
const fresh = key ? await fromApi(ids, key) : await fromPages(ids);
const views: Record<string, number> = { ...old.views };
const lengths: Record<string, number> = { ...old.seconds };
for (const [id, s] of Object.entries(fresh)) {
  if (s.views !== undefined) views[id] = Math.max(s.views, views[id] ?? 0);
  if (s.seconds) lengths[id] = s.seconds;
}

const byViews = (r: Record<string, number>) =>
  Object.fromEntries(
    Object.entries(r).sort(([a], [b]) => (views[b] ?? 0) - (views[a] ?? 0)),
  );
const updated = new Date().toISOString().slice(0, 10);
await writeFile(
  OUT,
  `${JSON.stringify({ seconds: byViews(lengths), updated, views: byViews(views) }, null, 2)}\n`,
);
const total = Object.values(views).reduce((a, b) => a + b, 0);
console.log(
  `Read ${Object.keys(fresh).length} of ${ids.length} videos; ${total.toLocaleString()} views in all. Wrote ${path.relative(process.cwd(), OUT)}.`,
);
