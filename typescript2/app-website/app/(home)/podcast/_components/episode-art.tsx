import { Play } from 'lucide-react';
import Image from 'next/image';
import type { PodcastEpisode } from '../podcast-data';

/** The video ID from a YouTube watch, short, or embed URL, or null if there isn't one. */
export const getYouTubeVideoId = (url: string) => {
  const match = url.match(
    /(?:youtu\.be\/|youtube\.com\/watch\?v=|youtube\.com\/embed\/)([^&\n?#]+)/,
  );
  return match ? match[1] : null;
};

/** The top of an episode card: the YouTube thumbnail with a play button, or, for an episode with no
 *  video yet, its number and title set on the page ground. Fills its (relative) parent. */
export function EpisodeArt({
  episode,
  isUpcoming,
  sizes,
}: {
  episode: PodcastEpisode;
  isUpcoming: boolean;
  sizes: string;
}) {
  const youtubeId = episode.youtubeUrl
    ? getYouTubeVideoId(episode.youtubeUrl)
    : null;

  if (youtubeId) {
    return (
      <>
        <Image
          alt={episode.title}
          className="object-cover"
          fill
          sizes={sizes}
          src={`https://img.youtube.com/vi/${youtubeId}/hqdefault.jpg`}
        />
        <div
          aria-hidden
          className="absolute inset-0 flex items-center justify-center bg-gradient-to-b from-black/5 to-black/40"
        >
          <span className="grid place-items-center size-11 rounded-full bg-accent text-on-accent transition-transform duration-200 group-hover:scale-110">
            <Play className="size-[18px] ml-0.5" fill="currentColor" />
          </span>
        </div>
      </>
    );
  }

  return (
    <div
      aria-hidden
      className="absolute inset-0 flex flex-col justify-between gap-4 bg-ground p-5"
    >
      <div className="v4-caption flex items-center gap-2 text-muted">
        <span className="text-clover">{episode.episodeNumber}</span>
        <span className="size-[3px] rounded-full bg-rule" />
        <span>{isUpcoming ? 'Live soon' : 'ai that works'}</span>
      </div>
      <div className="max-w-[92%] font-display font-medium text-[clamp(1.15rem,2vw,1.5rem)] leading-[1.2] tracking-[-0.02em] text-head">
        {episode.title}
      </div>
      <div className="h-0.5 w-16 bg-clover opacity-60" />
    </div>
  );
}
