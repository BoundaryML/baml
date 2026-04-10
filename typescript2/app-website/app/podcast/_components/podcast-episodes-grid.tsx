'use client';

import { formatDistanceToNow } from 'date-fns';
import { ArrowRight, Calendar, Code, Play } from 'lucide-react';
import Image from 'next/image';
import Link from 'next/link';
import { Card } from '@/components/ui/card';

interface PodcastEpisode {
  date: string;
  description: string;
  episodeNumber: string;
  featured: boolean;
  id: number;
  rsvpUrl?: string;
  title: string;
  topics: string[];
  codeUrl?: string;
  youtubeUrl?: string;
  slug: string;
}

interface PodcastEpisodesGridProps {
  episodes: PodcastEpisode[];
}

// Helper function to extract YouTube video ID from URL
const getYouTubeVideoId = (url: string) => {
  const match = url.match(
    /(?:youtu\.be\/|youtube\.com\/watch\?v=|youtube\.com\/embed\/)([^&\n?#]+)/,
  );
  return match ? match[1] : null;
};

export function PodcastEpisodesGrid({ episodes }: PodcastEpisodesGridProps) {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 sm:gap-6 auto-rows-fr">
      {episodes.map((episode) => {
        const isUpcoming = new Date(episode.date) > new Date();
        return (
        <Link href={`/podcast/${episode.slug}`} key={episode.id}>
          <Card className="overflow-hidden hover:shadow-lg transition-shadow cursor-pointer py-0 flex flex-col h-full gap-0">
          {/* Cover Image - YouTube Thumbnail */}
          {episode.youtubeUrl && getYouTubeVideoId(episode.youtubeUrl) && (
            <div className="relative h-40 sm:h-48">
              <Image
                alt={episode.title}
                className="object-cover w-full h-full"
                height={512}
                src={`https://img.youtube.com/vi/${getYouTubeVideoId(episode.youtubeUrl)}/0.jpg`}
                width={512}
              />
              {/* Play button overlay */}
              <div className="absolute inset-0 flex items-center justify-center bg-black/30 group-hover:bg-black/40 transition-colors">
                <div className="w-12 h-12 sm:w-16 sm:h-16 bg-red-600 rounded-full flex items-center justify-center group-hover:scale-110 transition-transform">
                  <Play
                    className="w-6 h-6 sm:w-8 sm:h-8 text-white ml-0.5 sm:ml-1"
                    fill="currentColor"
                  />
                </div>
              </div>
            </div>
          )}
          {!episode.youtubeUrl && isUpcoming && (
            <div className="relative h-40 sm:h-48 bg-gradient-to-br from-primary/15 to-transparent flex items-center justify-center">
              <div className="text-primary text-xs sm:text-sm font-medium flex items-center gap-2 bg-background/80 backdrop-blur px-3 py-1 rounded-full border border-primary/20">
                <span className="h-2 w-2 bg-primary rounded-full animate-pulse" />
                Upcoming • {formatDistanceToNow(new Date(episode.date), { addSuffix: true })}
              </div>
            </div>
          )}

          <div className="p-4 sm:p-6 flex flex-col flex-1">
            {/* Episode number and date */}
            <div className="flex items-center gap-2 sm:gap-3 text-xs text-muted-foreground mb-3">
              <span className="px-2 py-1 rounded-full text-xs font-medium bg-primary/10 text-primary">
                {episode.episodeNumber}
              </span>
              <div className="flex items-center gap-1">
                <Calendar className="h-3 w-3" />
                <span>
                  {formatDistanceToNow(new Date(episode.date), {
                    addSuffix: true,
                  })}
                </span>
              </div>
            </div>

            {/* Title */}
            <h3 className="text-lg sm:text-xl font-semibold mb-2 line-clamp-2">
              {episode.title}
            </h3>

            {/* Description */}
            <p className="text-muted-foreground text-sm mb-4 flex-1 line-clamp-3 sm:line-clamp-4">
              {episode.description}
            </p>

            {/* Topics */}
            <div className="flex flex-wrap gap-1 sm:gap-2 mb-4">
              {episode.topics.slice(0, 3).map((topic) => (
                <span
                  className="text-xs bg-muted px-2 py-1 rounded-full"
                  key={topic}
                >
                  {topic}
                </span>
              ))}
              {episode.topics.length > 3 && (
                <span className="text-xs text-muted-foreground">
                  +{episode.topics.length - 3} more
                </span>
              )}
            </div>

            {/* Bottom section with actions */}
            <div className="flex items-center justify-between mt-auto">
              <div className="flex items-center gap-2">
                {/* Code link if available */}
                {episode.codeUrl && !isUpcoming && (
                  <Link
                    className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                    href={episode.codeUrl}
                    onClick={(e) => e.stopPropagation()}
                    target="_blank"
                  >
                    <Code className="h-3 w-3" />
                    <span className="hidden sm:inline">Demo Code</span>
                    <span className="sm:hidden">Code</span>
                  </Link>
                )}

                {/* RSVP link if available */}
                {episode.rsvpUrl && !episode.youtubeUrl && (
                  <Link
                    className="flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                    href={episode.rsvpUrl}
                    onClick={(e) => e.stopPropagation()}
                    target="_blank"
                  >
                    <Calendar className="h-3 w-3" />
                    <span>RSVP</span>
                  </Link>
                )}
              </div>

              <div className="flex items-center gap-1 text-muted-foreground">
                <span className="text-sm">{episode.youtubeUrl ? 'Watch' : (isUpcoming ? 'RSVP' : 'View')}</span>
                <ArrowRight className="h-3 w-3" />
              </div>
            </div>
          </div>
        </Card>
        </Link>
        );
      })}
    </div>
  );
}
