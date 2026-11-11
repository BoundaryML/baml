import { createMetadata } from '@/app/_lib/metadata';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { Button, LINKS } from '@/components/home/ui';
import { CALENDAR, HeroSection, TWITCH } from './_components/hero-section';
import {
  EpisodeCard,
  PodcastEpisodesGrid,
} from './_components/podcast-episodes-grid';
import { fallbackEpisodes, fetchPodcastEpisodes } from './podcast-data';
import { formatViews } from './youtube-views';

export const metadata = createMetadata({
  description:
    'ai that works: weekly live sessions building real AI applications with BAML.',
  keywords:
    'AI podcast, LLM development, BAML, AI engineering, machine learning, developer podcast, AI applications',
  ogTitle: 'ai that works',
  path: '/podcast',
  podcast: true,
  title: 'ai that works',
  titleAbsolute:
    '🦄 ai that works: Weekly AI Development Sessions | BAML Podcast',
});

/** The podcast index, in the homepage's BAML lane: the show and its numbers, the most-watched
 *  episodes, then every episode. The nav's Platform menu links it. */
export default async function PodcastPage() {
  const fetched = await fetchPodcastEpisodes();
  const episodes = (fetched.length > 0 ? fetched : fallbackEpisodes).filter(
    (ep) => !ep.featured,
  );
  const total = episodes.reduce((n, ep) => n + (ep.views ?? 0), 0);
  const first = episodes.reduce(
    (min, ep) => (ep.date < min ? ep.date : min),
    episodes[0]?.date ?? '',
  );
  const since = first
    ? new Date(first).toLocaleDateString('en-US', {
        month: 'long',
        timeZone: 'UTC',
        year: 'numeric',
      })
    : '';
  const popular = [...episodes]
    .filter((ep) => ep.views !== undefined)
    .sort((a, b) => (b.views ?? 0) - (a.views ?? 0))
    .slice(0, 3);

  return (
    <PageShell>
      <HeroSection
        episodes={episodes.length}
        since={since}
        views={formatViews(total)}
      />

      {popular.length > 0 && (
        <section
          {...section('podcast_popular')}
          aria-labelledby="popular"
          className={`${PAGE_WRAP} pb-16 md:pb-20`}
        >
          <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1">
            <h2 className="v4-h3 text-head" id="popular">
              Most watched
            </h2>
            <p className="text-[13px] text-muted">
              Sorry about the thumbnails. YouTube demands it.
            </p>
          </div>
          <ul className="mt-6 grid gap-x-5 gap-y-9 md:grid-cols-3">
            {popular.map((ep) => (
              <li key={ep.id}>
                <EpisodeCard episode={ep} large />
              </li>
            ))}
          </ul>
        </section>
      )}

      <section
        {...section('podcast_episodes')}
        aria-label="Episodes"
        className={`${PAGE_WRAP} border-t border-rule pt-14 pb-16 md:pt-16 md:pb-24`}
      >
        <PodcastEpisodesGrid episodes={episodes} />
      </section>

      <section
        {...section('podcast_subscribe')}
        aria-labelledby="subscribe"
        className="border-t border-rule"
      >
        <div className={`${PAGE_WRAP} py-20 md:py-28`}>
          <h2 className="v4-h2 max-w-[14em]" id="subscribe">
            Never miss an episode
          </h2>
          <p className="mt-6 max-w-[36rem] v4-body text-ink-2">
            Join live on Twitch every week, or catch the recording on YouTube.
          </p>
          <div className="mt-8 flex flex-wrap gap-2.5">
            <Button href={CALENDAR}>Add to your calendar</Button>
            <Button href={TWITCH} variant="secondary">
              Follow on Twitch
            </Button>
            <Button href={LINKS.youtube} variant="secondary">
              Subscribe on YouTube
            </Button>
            <Button href="/discord" variant="secondary">
              Join the Discord
            </Button>
          </div>
        </div>
      </section>
    </PageShell>
  );
}
