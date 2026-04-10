'use client';

import { formatDistanceToNow } from 'date-fns';
import { Calendar, Play } from 'lucide-react';
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

interface EpisodesCarouselProps {
  episodes: PodcastEpisode[];
  currentSlug?: string;
}

// Helper function to extract YouTube video ID from URL
const getYouTubeVideoId = (url: string) => {
  const match = url.match(
    /(?:youtu\.be\/|youtube\.com\/watch\?v=|youtube\.com\/embed\/)([^&\n?#]+)/,
  );
  return match ? match[1] : null;
};

export function EpisodesCarousel({ episodes, currentSlug }: EpisodesCarouselProps) {
  const filteredEpisodes = episodes.filter(ep => ep.slug !== currentSlug);

  return (
    <div className="w-full">
      <h2 className="text-2xl font-semibold mb-6">All Episodes</h2>
      <div className="relative">
        <div className="flex gap-4 overflow-x-auto pb-4 scrollbar-hide">
          {filteredEpisodes.map((episode) => {
            const isUpcoming = new Date(episode.date) > new Date();
            
            return (
              <Link 
                href={`/podcast/${episode.slug}`} 
                key={episode.id}
                className="flex-shrink-0 w-80"
              >
                <Card className="overflow-hidden hover:shadow-lg transition-shadow cursor-pointer h-full">
                  {/* Cover Image - YouTube Thumbnail */}
                  {episode.youtubeUrl && getYouTubeVideoId(episode.youtubeUrl) && (
                    <div className="relative h-40">
                      <Image
                        alt={episode.title}
                        className="object-cover w-full h-full"
                        height={160}
                        src={`https://img.youtube.com/vi/${getYouTubeVideoId(episode.youtubeUrl)}/0.jpg`}
                        width={320}
                      />
                      {/* Play button overlay */}
                      <div className="absolute inset-0 flex items-center justify-center bg-black/30 group-hover:bg-black/40 transition-colors">
                        <div className="w-12 h-12 bg-red-600 rounded-full flex items-center justify-center group-hover:scale-110 transition-transform">
                          <Play
                            className="w-6 h-6 text-white ml-0.5"
                            fill="currentColor"
                          />
                        </div>
                      </div>
                    </div>
                  )}
                  {!episode.youtubeUrl && isUpcoming && (
                    <div className="relative h-40 bg-gradient-to-br from-primary/15 to-transparent flex items-center justify-center">
                      <div className="text-primary text-xs font-medium flex items-center gap-2 bg-background/80 backdrop-blur px-3 py-1 rounded-full border border-primary/20">
                        <span className="h-2 w-2 bg-primary rounded-full animate-pulse" />
                        Upcoming
                      </div>
                    </div>
                  )}

                  <div className="p-4">
                    {/* Episode number and date */}
                    <div className="flex items-center gap-2 text-xs text-muted-foreground mb-2">
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
                    <h3 className="text-base font-semibold mb-2 line-clamp-2">
                      {episode.title}
                    </h3>

                    {/* Description */}
                    <p className="text-muted-foreground text-sm line-clamp-3">
                      {episode.description}
                    </p>
                  </div>
                </Card>
              </Link>
            );
          })}
        </div>
      </div>
    </div>
  );
}
