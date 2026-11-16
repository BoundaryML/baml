import { formatDistanceToNow } from 'date-fns';
import { ArrowLeft, Calendar, Code } from 'lucide-react';
import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { ogImagePath, TWITTER_HANDLE } from '@/app/_lib/metadata';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { Button } from '@/components/home/ui';
import { Markdown } from '@/components/magicui/markdown';
import { ArticleStructuredData } from '@/components/structured-data';
import { getYouTubeVideoId } from '../_components/episode-art';
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

/** The slug of every episode, so each episode page is built ahead of time. */
export async function generateStaticParams(): Promise<{ slug: string }[]> {
  const episodes = await fetchPodcastEpisodes();
  return episodes.map((ep) => ({ slug: ep.slug }));
}

/**
 * Fetches an episode's README from GitHub as Markdown, with HTML images
 * converted, the episode title and YouTube links removed, and blank lines
 * collapsed. Returns an empty string if it can't be fetched.
 */
async function fetchReadmeFromGitHub(
  codeUrl: string,
  episodeTitle: string,
): Promise<string> {
  try {
    // Convert GitHub tree URL to raw README URL
    const rawUrl = `${codeUrl
      .replace('github.com', 'raw.githubusercontent.com')
      .replace('/tree/', '/')}/README.md`;

    const response = await fetch(rawUrl, { signal: AbortSignal.timeout(8000) });
    if (!response.ok) {
      // Try README.md with different casing
      const altUrl = `${codeUrl
        .replace('github.com', 'raw.githubusercontent.com')
        .replace('/tree/', '/')}/readme.md`;

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

/**
 * The episode page's title, description, Open Graph, and Twitter card
 * metadata. Next 15: `params` is a Promise, so it must be awaited before
 * access.
 */
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

/**
 * An episode page: the video, the description, the README when the episode
 * has code, its topics, and a carousel of other episodes.
 */
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
    <PageShell>
      <ArticleStructuredData
        datePublished={episode.date}
        description={episode.description}
        headline={`ai that works: ${episode.title}`}
        image={ogImagePath({
          description: episode.description,
          title: episode.title,
        })}
        url={`/podcast/${episode.slug}`}
      />

      <article
        {...section('podcast_episode')}
        className={`${PAGE_WRAP} pt-10 md:pt-14 pb-16 md:pb-24`}
      >
        <div className="mx-auto max-w-[56rem]">
          <Link
            className="tap inline-flex items-center gap-2 text-[15px] text-body hover:text-head transition-colors"
            href="/podcast"
          >
            <ArrowLeft aria-hidden className="size-4" />
            Back to episodes
          </Link>

          <header className="mt-10 md:mt-12">
            <div className="v4-caption flex items-center gap-3 text-muted">
              <span className="text-clover">{episode.episodeNumber}</span>
              <span className="inline-flex items-center gap-1.5">
                <Calendar aria-hidden className="size-4" />
                {formatDistanceToNow(new Date(episode.date), {
                  addSuffix: true,
                })}
              </span>
            </div>
            <h1 className="mt-5 v4-h2">{episode.title}</h1>
            <p className="mt-6 v4-body text-ink-2">{episode.description}</p>
          </header>

          {videoId && (
            <div className="mt-10 aspect-video w-full overflow-hidden rounded-2xl border border-rule bg-raised">
              <iframe
                allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
                allowFullScreen
                className="size-full"
                src={`https://www.youtube.com/embed/${videoId}`}
                title={episode.title}
              />
            </div>
          )}

          {isUpcoming && !videoId && (
            <div className="mt-10 rounded-2xl border border-rule bg-raised p-6">
              <p className="v4-caption flex items-center gap-2 text-clover">
                <span
                  aria-hidden
                  className="size-2 rounded-full bg-clover animate-pulse"
                />
                Upcoming event
              </p>
              <p className="mt-3 text-[16px] leading-[1.5] text-body">
                This episode hasn't aired yet. RSVP to get notified when it goes
                live.
              </p>
            </div>
          )}

          {readmeContent && (
            <section aria-labelledby="details" className="mt-14 md:mt-16">
              <div className="flex flex-wrap items-center justify-between gap-4 pb-5 border-b border-rule">
                <h2
                  className="font-display font-medium text-[26px] md:text-[32px] leading-[1.2] tracking-[-0.02em] text-head"
                  id="details"
                >
                  Project details
                </h2>
                {episode.codeUrl && (
                  <Button href={episode.codeUrl} variant="secondary">
                    <Code aria-hidden className="mr-2 size-4" />
                    Open in GitHub
                  </Button>
                )}
              </div>
              <Markdown
                className="mt-8 prose blog-prose"
                components={{
                  img: ({ src, alt, ...props }) => (
                    // biome-ignore lint/performance/noImgElement: README images come from arbitrary hosts that next/image isn't configured for.
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
            </section>
          )}

          {episode.topics.length > 0 && (
            <section aria-labelledby="topics" className="mt-14 md:mt-16">
              <h2 className="v4-caption text-muted" id="topics">
                Topics
              </h2>
              <ul className="mt-4 flex flex-wrap gap-2.5">
                {episode.topics.map((topic) => (
                  <li
                    className="rounded-full border border-rule px-4 py-2 text-[14px] text-ink-2"
                    key={topic}
                  >
                    {topic}
                  </li>
                ))}
              </ul>
            </section>
          )}

          <div className="mt-10 flex flex-wrap gap-2.5 border-t border-rule pt-8">
            {episode.codeUrl && !isUpcoming && (
              <Button href={episode.codeUrl} variant="secondary">
                <Code aria-hidden className="mr-2 size-4" />
                Demo code
              </Button>
            )}
            {episode.rsvpUrl && isUpcoming && (
              <Button href={episode.rsvpUrl}>
                <Calendar aria-hidden className="mr-2 size-4" />
                RSVP
              </Button>
            )}
            {episode.youtubeUrl && !isUpcoming && (
              <Button href={episode.youtubeUrl} variant="secondary">
                Watch on YouTube
              </Button>
            )}
          </div>
        </div>
      </article>

      <section
        {...section('podcast_more_episodes')}
        className="border-t border-rule"
      >
        <div className={`${PAGE_WRAP} py-16 md:py-20`}>
          <EpisodesCarousel currentSlug={episode.slug} episodes={episodes} />
        </div>
      </section>
    </PageShell>
  );
}
