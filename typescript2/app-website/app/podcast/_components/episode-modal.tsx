'use client';

import { useEffect, useState } from 'react';
import { formatDistanceToNow } from 'date-fns';
import { Calendar, Code } from 'lucide-react';
import Link from 'next/link';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Markdown } from '@/components/magicui/markdown';


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
}

interface EpisodeModalProps {
  episode: PodcastEpisode | null;
  isOpen: boolean;
  onClose: () => void;
}

// Helper function to extract YouTube video ID from URL
const getYouTubeVideoId = (url: string) => {
  const match = url.match(
    /(?:youtu\.be\/|youtube\.com\/watch\?v=|youtube\.com\/embed\/)([^&\n?#]+)/,
  );
  return match ? match[1] : null;
};

// Helper function to fetch README from GitHub
async function fetchReadmeFromGitHub(codeUrl: string): Promise<string> {
  try {
    // Convert GitHub tree URL to raw README URL
    const rawUrl = codeUrl
      .replace('github.com', 'raw.githubusercontent.com')
      .replace('/tree/', '/')
      + '/README.md';
    
    const response = await fetch(rawUrl);
    if (!response.ok) {
      // Try README.md with different casing
      const altUrl = codeUrl
        .replace('github.com', 'raw.githubusercontent.com')
        .replace('/tree/', '/')
        + '/readme.md';
      
      const altResponse = await fetch(altUrl);
      if (!altResponse.ok) {
        throw new Error('README not found');
      }
      return await altResponse.text();
    }
    
    const content = await response.text();
    
    // Convert HTML img tags to markdown image syntax
    const processedContent = content.replace(
      /<img[^>]+src="([^"]+)"[^>]*>/g,
      (match, src) => {
        // Extract alt text if available
        const altMatch = match.match(/alt="([^"]*)"/);
        const alt = altMatch ? altMatch[1] : 'image';
        return `![${alt}](${src})`;
      }
    );
    
    return processedContent;
  } catch (error) {
    console.error('Failed to fetch README:', error);
    return '';
  }
}

export function EpisodeModal({ episode, isOpen, onClose }: EpisodeModalProps) {
  const [readmeContent, setReadmeContent] = useState<string>('');
  const [isLoadingReadme, setIsLoadingReadme] = useState(false);

  useEffect(() => {
    if (episode?.codeUrl && isOpen) {
      setIsLoadingReadme(true);
      fetchReadmeFromGitHub(episode.codeUrl)
        .then(setReadmeContent)
        .catch(() => setReadmeContent(''))
        .finally(() => setIsLoadingReadme(false));
    }
  }, [episode?.codeUrl, isOpen]);

  if (!episode) return null;

  const videoId = episode.youtubeUrl ? getYouTubeVideoId(episode.youtubeUrl) : null;
  const isUpcoming = new Date(episode.date) > new Date();

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <div className="flex items-start justify-between">
            <div className="flex-1">
              <DialogTitle className="text-xl font-semibold mb-2">
                🦄 {episode.title}
              </DialogTitle>
              <div className="flex items-center gap-3 text-sm text-muted-foreground mb-4">
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
            </div>

          </div>
        </DialogHeader>

        <div className="space-y-6">
          {/* Video Player */}
          {videoId && (
            <div className="aspect-video w-full">
              <iframe
                className="w-full h-full rounded-lg"
                src={`https://www.youtube.com/embed/${videoId}`}
                title={episode.title}
                allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
                allowFullScreen
              />
            </div>
          )}

          {/* Upcoming Event Banner */}
          {isUpcoming && !videoId && (
            <div className="bg-gradient-to-r from-primary/10 to-primary/5 border border-primary/20 rounded-lg p-4">
              <div className="flex items-center gap-2 text-primary font-medium">
                <span className="h-2 w-2 bg-primary rounded-full animate-pulse" />
                Upcoming Event
              </div>
              <p className="text-sm text-muted-foreground mt-1">
                This episode hasn't aired yet. RSVP to get notified when it goes live.
              </p>
            </div>
          )}

          {/* README Content */}
          {episode.codeUrl && (
            <div>
              <h3 className="font-semibold mb-2">Project Details</h3>
              {isLoadingReadme ? (
                <div className="text-muted-foreground">
                  <div className="animate-pulse space-y-2">
                    <div className="h-4 bg-muted rounded w-3/4" />
                    <div className="h-4 bg-muted rounded w-1/2" />
                    <div className="h-4 bg-muted rounded w-5/6" />
                  </div>
                </div>
              ) : readmeContent ? (
                <div className="prose prose-sm max-w-none">
                  <div className="text-sm text-muted-foreground">
                    <Markdown 
                      className="text-sm text-muted-foreground"
                      components={{
                        img: ({ src, alt, ...props }) => (
                          <img
                            src={src}
                            alt={alt}
                            className="max-w-full h-auto rounded-lg my-4"
                            loading="lazy"
                            {...props}
                          />
                        ),
                      }}
                    >
                      {readmeContent}
                    </Markdown>
                  </div>
                </div>
              ) : (
                <p className="text-muted-foreground text-sm">
                  No README found for this project.
                </p>
              )}
            </div>
          )}

          {/* Topics */}
          {episode.topics.length > 0 && (
            <div>
              <h3 className="font-semibold mb-2">Topics</h3>
              <div className="flex flex-wrap gap-2">
                {episode.topics.map((topic) => (
                  <span
                    key={topic}
                    className="text-xs bg-muted px-3 py-1 rounded-full"
                  >
                    {topic}
                  </span>
                ))}
              </div>
            </div>
          )}

          {/* Actions */}
          <div className="flex items-center gap-3 pt-4 border-t">
            {episode.codeUrl && !isUpcoming && (
              <Button asChild variant="outline" size="sm">
                <Link href={episode.codeUrl} target="_blank">
                  <Code className="h-4 w-4 mr-2" />
                  Demo Code
                </Link>
              </Button>
            )}
            {episode.rsvpUrl && isUpcoming && (
              <Button asChild size="sm">
                <Link href={episode.rsvpUrl} target="_blank">
                  <Calendar className="h-4 w-4 mr-2" />
                  RSVP
                </Link>
              </Button>
            )}
            {episode.youtubeUrl && !isUpcoming && (
              <Button asChild variant="outline" size="sm">
                <Link href={episode.youtubeUrl} target="_blank">
                  Watch on YouTube
                </Link>
              </Button>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
