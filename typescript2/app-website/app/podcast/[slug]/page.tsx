import { notFound } from 'next/navigation';
import { formatDistanceToNow } from 'date-fns';
import { Calendar, Code, ArrowLeft } from 'lucide-react';
import Link from 'next/link';
import { Button } from '@/components/ui/button';
import { Markdown } from '@/components/magicui/markdown';
import { Navbar } from '@/components/navbar';
import { FooterSection } from '@/components/footer-section';
import { EpisodesCarousel } from '../_components/episodes-carousel';
import { fetchPodcastEpisodes } from '../podcast-data';
import type { Metadata } from 'next';

// Helper function to extract YouTube video ID from URL
const getYouTubeVideoId = (url: string) => {
  const match = url.match(
    /(?:youtu\.be\/|youtube\.com\/watch\?v=|youtube\.com\/embed\/)([^&\n?#]+)/,
  );
  return match ? match[1] : null;
};

// Helper function to fetch README from GitHub
async function fetchReadmeFromGitHub(codeUrl: string, episodeTitle: string): Promise<string> {
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
    let processedContent = content.replace(
      /<img[^>]+src="([^"]+)"[^>]*>/g,
      (match, src) => {
        // Extract alt text if available
        const altMatch = match.match(/alt="([^"]*)"/);
        const alt = altMatch ? altMatch[1] : 'image';
        return `![${alt}](${src})`;
      }
    );
    
    // Clean up duplicate titles (remove markdown headers that match the episode title)
    const titleRegex = new RegExp(`^#+\\s*${episodeTitle.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*$`, 'gm');
    processedContent = processedContent.replace(titleRegex, '');
    
    // Remove YouTube video links and thumbnails
    processedContent = processedContent.replace(
      /\[!\[.*?\]\(https:\/\/img\.youtube\.com\/vi\/[^)]+\)\]\(https:\/\/www\.youtube\.com\/watch\?v=[^)]+\)/g,
      ''
    );
    
    // Remove standalone YouTube links
    processedContent = processedContent.replace(
      /\[Video\]\(https:\/\/www\.youtube\.com\/watch\?v=[^)]+\)\s*\([^)]*\)/g,
      ''
    );
    
    // Clean up extra whitespace and empty lines
    processedContent = processedContent
      .replace(/\n\s*\n\s*\n/g, '\n\n') // Remove multiple empty lines
      .trim();
    
    return processedContent;
  } catch (error) {
    console.error('Failed to fetch README:', error);
    return '';
  }
}

// Generate metadata for SEO
export async function generateMetadata({ params }: { params: { slug: string } }): Promise<Metadata> {
  const episodes = await fetchPodcastEpisodes();
  const episode = episodes.find(ep => ep.slug === params.slug);

  if (!episode) {
    return {
      title: 'Episode Not Found',
    };
  }

  const baseUrl = process.env.SITE_URL || 'https://boundaryml.com';
  const episodeUrl = `${baseUrl}/podcast/${episode.slug}`;

  // Extract YouTube thumbnail if available
  const videoId = episode.youtubeUrl ? getYouTubeVideoId(episode.youtubeUrl) : null;
  const thumbnailUrl = videoId
    ? `https://img.youtube.com/vi/${videoId}/maxresdefault.jpg`
    : `${baseUrl}/baml-og-background.png`; // Fallback to default image

  const fullTitle = `🦄 ai that works: ${episode.title} | BAML Podcast`;
  const fullDescription = `${episode.episodeNumber}: ${episode.description}`;

  return {
    title: fullTitle,
    description: fullDescription,
    alternates: {
      canonical: episodeUrl,
    },
    keywords: [...episode.topics, 'AI', 'LLM', 'BAML', 'Boundary ML', 'Podcast'].join(', '),
    openGraph: {
      title: fullTitle,
      description: fullDescription,
      url: episodeUrl,
      siteName: 'BAML',
      type: 'article',
      publishedTime: episode.date,
      images: [
        {
          url: thumbnailUrl,
          width: 1280,
          height: 720,
          alt: `${episode.title} - Episode ${episode.episodeNumber}`,
        },
      ],
      videos: videoId ? [
        {
          url: `https://www.youtube.com/watch?v=${videoId}`,
          secureUrl: `https://www.youtube.com/watch?v=${videoId}`,
          type: 'text/html',
          width: 1280,
          height: 720,
        },
      ] : undefined,
    },
    twitter: {
      card: 'summary_large_image',
      title: fullTitle,
      description: fullDescription,
      images: [thumbnailUrl],
      creator: '@boundaryml',
      site: '@boundaryml',
    },
  };
}

export default async function EpisodePage({ params }: { params: { slug: string } }) {
  const episodes = await fetchPodcastEpisodes();
  const episode = episodes.find(ep => ep.slug === params.slug);

  if (!episode) {
    notFound();
  }

  const videoId = episode.youtubeUrl ? getYouTubeVideoId(episode.youtubeUrl) : null;
  const isUpcoming = new Date(episode.date) > new Date();
  const readmeContent = episode.codeUrl && !isUpcoming ? await fetchReadmeFromGitHub(episode.codeUrl, `🦄 ai that works: ${episode.title}`) : '';

  return (
    <div className="max-w-7xl mx-auto border-x relative">
      <Navbar />
      <main className="min-h-screen w-full">
        {/* Back Button */}
        <div className="px-4 sm:px-6 py-4">
          <Button asChild variant="ghost" size="sm">
            <Link href="/podcast" className="flex items-center gap-2">
              <ArrowLeft className="h-4 w-4" />
              Back to Episodes
            </Link>
          </Button>
        </div>

        <div className="px-4 sm:px-6 pb-12">
          <div className="max-w-4xl mx-auto">
            {/* Header */}
            <div className="mb-8">
              <div className="flex items-center gap-3 text-sm text-muted-foreground mb-4">
                <span className="px-3 py-1 rounded-full text-sm font-medium bg-primary/10 text-primary">
                  {episode.episodeNumber}
                </span>
                <div className="flex items-center gap-1">
                  <Calendar className="h-4 w-4" />
                  <span>
                    {formatDistanceToNow(new Date(episode.date), {
                      addSuffix: true,
                    })}
                  </span>
                </div>
              </div>
              <h1 className="text-3xl sm:text-4xl font-bold mb-4">
                🦄 {episode.title}
              </h1>
              <p className="text-lg text-muted-foreground leading-relaxed">
                {episode.description}
              </p>
            </div>

            {/* Video Player */}
            {videoId && (
              <div className="mb-8">
                <div className="aspect-video w-full">
                  <iframe
                    className="w-full h-full rounded-lg"
                    src={`https://www.youtube.com/embed/${videoId}`}
                    title={episode.title}
                    allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
                    allowFullScreen
                  />
                </div>
              </div>
            )}

            {/* Upcoming Event Banner */}
            {isUpcoming && !videoId && (
              <div className="bg-gradient-to-r from-primary/10 to-primary/5 border border-primary/20 rounded-lg p-6 mb-8">
                <div className="flex items-center gap-2 text-primary font-medium mb-2">
                  <span className="h-2 w-2 bg-primary rounded-full animate-pulse" />
                  Upcoming Event
                </div>
                <p className="text-muted-foreground">
                  This episode hasn't aired yet. RSVP to get notified when it goes live.
                </p>
              </div>
            )}

            {/* README Content */}
            {readmeContent && (
              <div className="mb-8">
                <div className="flex items-center justify-between mb-6">
                  <h2 className="text-2xl font-semibold">Project Details</h2>
                  {episode.codeUrl && (
                    <Button asChild variant="outline" size="sm">
                      <Link href={episode.codeUrl} target="_blank">
                        <Code className="h-4 w-4 mr-2" />
                        Open in GitHub
                      </Link>
                    </Button>
                  )}
                </div>
                <div className="prose prose-lg max-w-none">
                  <Markdown 
                    className="text-base text-foreground"
                    components={{
                      img: ({ src, alt, ...props }) => (
                        <img
                          src={src}
                          alt={alt}
                          className="max-w-full h-auto rounded-lg my-6"
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
            )}

            {/* Topics */}
            {episode.topics.length > 0 && (
              <div className="mb-8">
                <h2 className="text-2xl font-semibold mb-4">Topics</h2>
                <div className="flex flex-wrap gap-3">
                  {episode.topics.map((topic) => (
                    <span
                      key={topic}
                      className="text-sm bg-muted px-4 py-2 rounded-full"
                    >
                      {topic}
                    </span>
                  ))}
                </div>
              </div>
            )}

            {/* Actions */}
            <div className="flex items-center gap-4 pt-8 border-t">
              {episode.codeUrl && !isUpcoming && (
                <Button asChild variant="outline" size="lg">
                  <Link href={episode.codeUrl} target="_blank">
                    <Code className="h-5 w-5 mr-2" />
                    Demo Code
                  </Link>
                </Button>
              )}
              {episode.rsvpUrl && isUpcoming && (
                <Button asChild size="lg">
                  <Link href={episode.rsvpUrl} target="_blank">
                    <Calendar className="h-5 w-5 mr-2" />
                    RSVP
                  </Link>
                </Button>
              )}
              {episode.youtubeUrl && !isUpcoming && (
                <Button asChild variant="outline" size="lg">
                  <Link href={episode.youtubeUrl} target="_blank">
                    Watch on YouTube
                  </Link>
                </Button>
              )}
            </div>
          </div>
        </div>

        {/* Episodes Carousel */}
        <div className="px-4 sm:px-6 py-12 bg-muted/30">
          <div className="max-w-4xl mx-auto">
            <EpisodesCarousel episodes={episodes} currentSlug={episode.slug} />
          </div>
        </div>
      </main>
      <FooterSection />
    </div>
  );
}
