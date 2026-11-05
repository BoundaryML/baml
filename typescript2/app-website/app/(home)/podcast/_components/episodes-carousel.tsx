'use client';

import { formatDistanceToNow } from 'date-fns';
import Link from 'next/link';
import type { PodcastEpisode } from '../podcast-data';
import { EpisodeArt } from './episode-art';

interface EpisodesCarouselProps {
  episodes: PodcastEpisode[];
  currentSlug?: string;
}

/** A horizontally scrolling row of episode cards, leaving out the episode at `currentSlug`. */
export function EpisodesCarousel({
  episodes,
  currentSlug,
}: EpisodesCarouselProps) {
  const filteredEpisodes = episodes.filter((ep) => ep.slug !== currentSlug);

  return (
    <div className="w-full">
      <h2 className="font-display font-medium text-[26px] md:text-[32px] leading-[1.2] tracking-[-0.02em] text-head">
        All episodes
      </h2>
      <ul className="mt-6 flex gap-4 overflow-x-auto pb-4 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
        {filteredEpisodes.map((episode) => {
          const isUpcoming = new Date(episode.date) > new Date();

          return (
            <li className="w-72 sm:w-80 shrink-0" key={episode.id}>
              <Link
                className="group flex h-full flex-col overflow-hidden rounded-2xl border border-rule bg-raised transition-colors hover:border-clover/50"
                href={`/podcast/${episode.slug}`}
              >
                <div className="relative aspect-video overflow-hidden border-b border-rule">
                  <EpisodeArt
                    episode={episode}
                    isUpcoming={isUpcoming}
                    sizes="320px"
                  />
                </div>

                <div className="flex flex-col gap-2.5 p-5">
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
                  <h3 className="line-clamp-2 font-display font-medium text-[18px] leading-[1.3] tracking-[-0.02em] text-head transition-colors group-hover:text-clover">
                    {episode.title}
                  </h3>
                  <p className="line-clamp-3 text-[14px] leading-[1.5] text-body">
                    {episode.description}
                  </p>
                </div>
              </Link>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
