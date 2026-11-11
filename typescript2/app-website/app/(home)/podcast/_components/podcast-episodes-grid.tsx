'use client';

import { formatDistanceToNow } from 'date-fns';
import Link from 'next/link';
import posthog from 'posthog-js';
import { useState } from 'react';
import type { PodcastEpisode } from '../podcast-data';
import { formatViews } from '../youtube-views';
import { EpisodeArt } from './episode-art';

// Every episode, the way YouTube lists videos: the thumbnail, the title, then its number, views and
// age. Sorted newest first, or by views; the most-watched order is the one that shows the show
// lands, the newest is what regulars come for.

type Sort = 'latest' | 'popular';

const SORTS: { key: Sort; label: string }[] = [
  { key: 'latest', label: 'Latest' },
  { key: 'popular', label: 'Most watched' },
];

/** One episode: its thumbnail, title, and the line under it. */
export function EpisodeCard({
  episode,
  large,
}: {
  episode: PodcastEpisode;
  large?: boolean;
}) {
  const isUpcoming = new Date(episode.date) > new Date();
  return (
    <Link
      className="group flex h-full flex-col"
      href={`/podcast/${episode.slug}`}
      onClick={() =>
        posthog.capture('podcast_episode_clicked', {
          episode_id: episode.id,
          episode_number: episode.episodeNumber,
          episode_title: episode.title,
          is_upcoming: isUpcoming,
        })
      }
    >
      <div className="relative aspect-video overflow-hidden rounded-xl border border-rule bg-raised transition-colors group-hover:border-clover/50">
        <EpisodeArt
          episode={episode}
          isUpcoming={isUpcoming}
          sizes={large ? '(max-width: 720px) 100vw, 600px' : '400px'}
        />
      </div>
      <h3
        className={`mt-3 line-clamp-2 font-display font-medium leading-[1.3] tracking-[-0.01em] text-head transition-colors group-hover:text-clover ${large ? 'text-[19px]' : 'text-[16px]'}`}
      >
        {episode.title}
      </h3>
      <p className="mt-1.5 flex flex-wrap items-center gap-x-2 text-[13px] text-muted">
        <span className="text-clover">{episode.episodeNumber}</span>
        {episode.views !== undefined && (
          <>
            <Dot />
            <span className="tabular-nums">
              {formatViews(episode.views)} views
            </span>
          </>
        )}
        <Dot />
        <span>
          {isUpcoming
            ? 'Upcoming'
            : formatDistanceToNow(new Date(episode.date), { addSuffix: true })}
        </span>
      </p>
    </Link>
  );
}

function Dot() {
  return <span aria-hidden className="size-[3px] rounded-full bg-rule" />;
}

/** Every episode, newest first or most watched first. */
export function PodcastEpisodesGrid({
  episodes,
}: {
  episodes: PodcastEpisode[];
}) {
  const [sort, setSort] = useState<Sort>('latest');
  const shown =
    sort === 'latest'
      ? episodes
      : [...episodes].sort((a, b) => (b.views ?? -1) - (a.views ?? -1));
  return (
    <div>
      <div className="flex flex-wrap items-end justify-between gap-4">
        <h2 className="v4-h3 text-head">All episodes</h2>
        <div className="flex gap-1 rounded-lg border border-rule p-1 text-[13px]">
          {SORTS.map((s) => (
            <button
              aria-pressed={sort === s.key}
              className={`cursor-pointer rounded-md px-3 py-1.5 transition-colors ${sort === s.key ? 'bg-white/[0.08] text-head' : 'text-muted hover:text-ink'}`}
              key={s.key}
              onClick={() => setSort(s.key)}
              type="button"
            >
              {s.label}
            </button>
          ))}
        </div>
      </div>
      <ul className="mt-6 grid gap-x-5 gap-y-9 grid-cols-[repeat(auto-fill,minmax(min(100%,260px),1fr))]">
        {shown.map((episode) => (
          <li key={episode.id}>
            <EpisodeCard episode={episode} />
          </li>
        ))}
      </ul>
    </div>
  );
}
