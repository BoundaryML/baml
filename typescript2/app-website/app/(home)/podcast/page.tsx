import { createMetadata } from '@/app/_lib/metadata';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { Button } from '@/components/home/ui';
import { HeroSection } from './_components/hero-section';
import { PodcastEpisodesGrid } from './_components/podcast-episodes-grid';
import { fallbackEpisodes, fetchPodcastEpisodes } from './podcast-data';

// The live sessions are the main thing to subscribe to, so the event calendar gets the accent fill.
const podcastPlatforms = [
  { label: 'Subscribe on the event calendar', url: 'https://lu.ma/baml' },
  { label: 'Subscribe on Discord', url: 'https://boundaryml.com/discord' },
  { label: 'Subscribe on GitHub', url: 'https://github.com/boundaryml/baml' },
  { label: 'Subscribe on YouTube', url: 'https://www.youtube.com/@boundaryml' },
];

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

/** The podcast index, in the homepage's BAML lane. The nav's Platform menu links it. */
export default async function PodcastPage() {
  const fetched = await fetchPodcastEpisodes();
  const podcastEpisodes = fetched.length > 0 ? fetched : fallbackEpisodes;
  return (
    <PageShell>
      <HeroSection />

      <section
        {...section('podcast_episodes')}
        aria-label="Episodes"
        className={`${PAGE_WRAP} pb-16 md:pb-24`}
      >
        <PodcastEpisodesGrid
          episodes={podcastEpisodes.filter((ep) => !ep.featured)}
        />
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
            Join our weekly sessions and learn how to build AI that actually
            works in production.
          </p>
          <div className="mt-8 flex flex-wrap gap-2.5">
            {podcastPlatforms.map((platform, i) => (
              <Button
                href={platform.url}
                key={platform.url}
                variant={i === 0 ? 'primary' : 'secondary'}
              >
                {platform.label}
              </Button>
            ))}
          </div>
        </div>
      </section>
    </PageShell>
  );
}
