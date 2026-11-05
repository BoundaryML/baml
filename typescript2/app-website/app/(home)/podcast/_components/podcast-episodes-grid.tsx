'use client';

import { formatDistanceToNow } from 'date-fns';
import { ArrowRight, Calendar, Code } from 'lucide-react';
import Link from 'next/link';
import posthog from 'posthog-js';
import type { PodcastEpisode } from '../podcast-data';
import { EpisodeArt } from './episode-art';

interface PodcastEpisodesGridProps {
  episodes: PodcastEpisode[];
}

/** A grid of episode cards, each linking to its episode page and logging the click. */
export function PodcastEpisodesGrid({ episodes }: PodcastEpisodesGridProps) {
  return (
    <ul className="grid gap-4 md:gap-5 grid-cols-[repeat(auto-fill,minmax(min(100%,320px),1fr))]">
      {episodes.map((episode) => {
        const isUpcoming = new Date(episode.date) > new Date();

        return (
          <li key={episode.id}>
            <Link
              className="group flex h-full flex-col overflow-hidden rounded-2xl border border-rule bg-raised transition-colors hover:border-clover/50"
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
              <div className="relative aspect-video overflow-hidden border-b border-rule">
                <EpisodeArt
                  episode={episode}
                  isUpcoming={isUpcoming}
                  sizes="(max-width: 720px) 100vw, 400px"
                />
              </div>

              <div className="flex flex-1 flex-col gap-4 px-5 pt-5 pb-4 md:px-6 md:pt-6">
                <div className="v4-caption flex items-center gap-2.5 text-muted">
                  <span className="text-clover">{episode.episodeNumber}</span>
                  <span
                    aria-hidden
                    className="size-[3px] rounded-full bg-rule"
                  />
                  <span>
                    {formatDistanceToNow(new Date(episode.date), {
                      addSuffix: true,
                    })}
                  </span>
                </div>

                <p className="line-clamp-3 text-[15px] leading-[1.5] text-body">
                  {episode.description}
                </p>

                <div className="mt-auto flex flex-wrap gap-2">
                  {episode.topics.slice(0, 3).map((topic) => (
                    <span
                      className="rounded-full border border-rule px-2.5 py-1 text-[12px] text-body"
                      key={topic}
                    >
                      {topic}
                    </span>
                  ))}
                </div>

                <div className="flex items-center justify-between border-t border-rule pt-4 text-[13px]">
                  <div className="flex gap-3 text-muted">
                    {episode.codeUrl && !isUpcoming && (
                      <span className="inline-flex items-center gap-1.5">
                        <Code aria-hidden className="size-3.5" />
                        Code
                      </span>
                    )}
                    {episode.rsvpUrl && isUpcoming && (
                      <span className="inline-flex items-center gap-1.5">
                        <Calendar aria-hidden className="size-3.5" />
                        RSVP
                      </span>
                    )}
                  </div>
                  <span className="inline-flex items-center gap-1 font-medium text-head transition-colors group-hover:text-clover">
                    {episode.youtubeUrl
                      ? 'Watch'
                      : isUpcoming
                        ? 'RSVP'
                        : 'View'}
                    <ArrowRight aria-hidden className="size-3.5" />
                  </span>
                </div>
              </div>
            </Link>
          </li>
        );
      })}
    </ul>
  );
}
