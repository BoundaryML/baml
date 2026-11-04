import { formatDistanceToNow } from 'date-fns';
import { ArrowLeft, Calendar, Code } from 'lucide-react';
import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { ogImagePath, TWITTER_HANDLE } from '@/app/_lib/metadata';
import { FooterSection } from '@/components/footer-section';
import { Markdown } from '@/components/magicui/markdown';
import { Navbar } from '@/components/navbar';
import { ArticleStructuredData } from '@/components/structured-data';
import { Button } from '@/components/ui/button';
import { EpisodesCarousel } from '../_components/episodes-carousel';
import { fetchPodcastEpisodes } from '../podcast-data';

// Pre-render every episode page at build time so each is served as static HTML
// from the CDN, never as a serverless function. Dynamic rendering hit this
// monorepo's file-tracing bug ("Cannot find module
// next/dist/compiled/source-map"), 500ing every episode page. force-static also
// makes the README fetches below cacheable, which is what was forcing this route
// dynamic in the first place.
export const dynamic = 'force-static';
export const dynamicParams = false;

export async function generateStaticParams(): Promise<{ slug: string }[]> {
  const episodes = await fetchPodcastEpisodes();
  return episodes.map((ep) => ({ slug: ep.slug }));
}

// Helper function to extract YouTube video ID from URL
const getYouTubeVideoId = (url: string) => {
  const match = url.match(
    /(?:youtu\.be\/|youtube\.com\/watch\?v=|youtube\.com\/embed\/)([^&\n?#]+)/,
  );
  return match ? match[1] : null;
};

// Helper function to fetch README from GitHub
async function fetchReadmeFromGitHub(
  codeUrl: string,
  episodeTitle: string,
): Promise<string> {
  try {
    // Convert GitHub tree URL to raw README URL
    const rawUrl =
      codeUrl
        .replace('github.com', 'raw.githubusercontent.com')
        .replace('/tree/', '/') + '/README.md';

    const response = await fetch(rawUrl, { signal: AbortSignal.timeout(8000) });
    if (!response.ok) {
      // Try README.md with different casing
      const altUrl =
        codeUrl
          .replace('github.com', 'raw.githubusercontent.com')
          .replace('/tree/', '/') + '/readme.md';

      const altResponse = await fetch(altUrl, {
        signal: AbortSignal.timeout(8000),
      });
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
      },
    );

    // Clean up duplicate titles (remove markdown headers that match the episode title)
    const titleRegex = new RegExp(
      `^#+\\s*${episodeTitle.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*$`,
      'gm',
    );
    processedContent = processedContent.replace(titleRegex, '');

    // Remove YouTube video links and thumbnails
    processedContent = processedContent.replace(
      /\[!\[.*?\]\(https:\/\/img\.youtube\.com\/vi\/[^)]+\)\]\(https:\/\/www\.youtube\.com\/watch\?v=[^)]+\)/g,
      '',
    );

    // Remove standalone YouTube links
    processedContent = processedContent.replace(
      /\[Video\]\(https:\/\/www\.youtube\.com\/watch\?v=[^)]+\)\s*\([^)]*\)/g,
      '',
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

// Generate metadata for SEO.
// Next 15: `params` is a Promise — must be awaited before access.
export async function generateMetadata({
  params,
}: {
  params: Promise<{ slug: string }>;
}): Promise<Metadata> {
  const { slug } = await params;
  const episodes = await fetchPodcastEpisodes();
  const episode = episodes.find((ep) => ep.slug === slug);

  if (!episode) {
    return {
      title: 'Episode Not Found',
    };
  }

  const baseUrl =
    process.env.NEXT_PUBLIC_BASE_URL ??
    (process.env.VERCEL_PROJECT_PRODUCTION_URL
      ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
      : 'http://localhost:3000');
  const episodeUrl = `${baseUrl}/podcast/${episode.slug}`;

  const videoId = episode.youtubeUrl
    ? getYouTubeVideoId(episode.youtubeUrl)
    : null;

  const fullTitle = `🦄 ai that works: ${episode.title} | BAML Podcast`;
  const cardTitle = `ai that works: ${episode.title}`;
  const fullDescription = `${episode.episodeNumber}: ${episode.description}`;

  // On-brand link-preview card (matches every other page).
  const ogImage = ogImagePath({
    description: episode.description,
    eyebrow: 'ai that works',
    title: episode.title,
  });

  return {
    alternates: {
      canonical: episodeUrl,
    },
    description: fullDescription,
    keywords: [
      ...episode.topics,
      'AI',
      'LLM',
      'BAML',
      'Boundary',
      'Podcast',
    ].join(', '),
    openGraph: {
      description: fullDescription,
      images: [
        {
          alt: `${episode.title} — ai that works`,
          height: 630,
          url: ogImage,
          width: 1200,
        },
      ],
      publishedTime: episode.date,
      siteName: 'BAML',
      title: cardTitle,
      type: 'article',
      url: episodeUrl,
      videos: videoId
        ? [
            {
              height: 720,
              secureUrl: `https://www.youtube.com/watch?v=${videoId}`,
              type: 'text/html',
              url: `https://www.youtube.com/watch?v=${videoId}`,
              width: 1280,
            },
          ]
        : undefined,
    },
    title: fullTitle,
    twitter: {
      card: 'summary_large_image',
      creator: TWITTER_HANDLE,
      description: fullDescription,
      images: [ogImage],
      site: TWITTER_HANDLE,
      title: cardTitle,
    },
  };
}

export default async function EpisodePage({
  params,
}: {
  params: Promise<{ slug: string }>;
}) {
  const { slug } = await params;
  const episodes = await fetchPodcastEpisodes();
  const episode = episodes.find((ep) => ep.slug === slug);

  if (!episode) {
    notFound();
  }

  const videoId = episode.youtubeUrl
    ? getYouTubeVideoId(episode.youtubeUrl)
    : null;
  const isUpcoming = new Date(episode.date) > new Date();
  const readmeContent =
    episode.codeUrl && !isUpcoming
      ? await fetchReadmeFromGitHub(
          episode.codeUrl,
          `🦄 ai that works: ${episode.title}`,
        )
      : '';

  return (
    <div className="max-w-7xl mx-auto border-x relative">
      <ArticleStructuredData
        datePublished={episode.date}
        description={episode.description}
        headline={`ai that works: ${episode.title}`}
        image={ogImagePath({
          description: episode.description,
          eyebrow: 'ai that works',
          title: episode.title,
        })}
        url={`/podcast/${episode.slug}`}
      />
      <Navbar />
      <main className="min-h-screen w-full">
        {/* Back Button */}
        <div className="px-4 sm:px-6 py-4">
          <Button asChild size="sm" variant="ghost">
            <Link className="flex items-center gap-2" href="/podcast">
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
                    allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
                    allowFullScreen
                    className="w-full h-full rounded-lg"
                    src={`https://www.youtube.com/embed/${videoId}`}
                    title={episode.title}
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
                  This episode hasn't aired yet. RSVP to get notified when it
                  goes live.
                </p>
              </div>
            )}

            {/* README Content */}
            {readmeContent && (
              <div className="mb-8">
                <div className="flex items-center justify-between mb-6">
                  <h2 className="text-2xl font-semibold">Project Details</h2>
                  {episode.codeUrl && (
                    <Button asChild size="sm" variant="outline">
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
                          alt={alt}
                          className="max-w-full h-auto rounded-lg my-6"
                          loading="lazy"
                          src={src}
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
                      className="text-sm bg-muted px-4 py-2 rounded-full"
                      key={topic}
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
                <Button asChild size="lg" variant="outline">
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
                <Button asChild size="lg" variant="outline">
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
            <EpisodesCarousel currentSlug={episode.slug} episodes={episodes} />
          </div>
        </div>
      </main>
      <FooterSection />
    </div>
  );
}
