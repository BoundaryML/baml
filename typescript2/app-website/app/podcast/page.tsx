import { Headphones } from 'lucide-react';
import Link from 'next/link';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import { Button } from '@/components/ui/button';
import { HeroSection } from './_components/hero-section';
import { PodcastEpisodesGrid } from './_components/podcast-episodes-grid';
import { fetchPodcastEpisodes, fallbackEpisodes } from './podcast-data';
import type { Metadata } from 'next';

const podcastPlatforms = [
  { icon: '📅', name: 'Event Calendar', url: 'https://lu.ma/baml' },
  { icon: '💬', name: 'Discord', url: 'https://boundaryml.com/discord' },
  { icon: '🚀', name: 'GitHub', url: 'https://github.com/boundaryml/baml' },
  { icon: '📺', name: 'YouTube', url: 'https://www.youtube.com/@boundaryml' },
];

export const metadata: Metadata = {
  title: '🦄 ai that works: Weekly AI Development Sessions | BAML Podcast',
  description:
    'Join our weekly interactive sessions where we build real AI applications together. Learn practical techniques for building production-ready AI systems with BAML.',
  alternates: {
    canonical: 'https://boundaryml.com/podcast',
  },
  keywords:
    'AI podcast, LLM development, BAML, AI engineering, machine learning, developer podcast, AI applications',
  openGraph: {
    title: '🦄 ai that works: Weekly AI Development Sessions',
    description:
      'Join our weekly interactive sessions where we build real AI applications together. Learn practical techniques for building production-ready AI systems.',
    url: 'https://boundaryml.com/podcast',
    siteName: 'BAML',
    type: 'website',
    images: [
      {
        url: 'https://boundaryml.com/baml-og-background.png',
        width: 1200,
        height: 630,
        alt: 'AI That Works - Weekly AI Development Sessions',
      },
    ],
  },
  twitter: {
    card: 'summary_large_image',
    title: '🦄 ai that works: Weekly AI Development Sessions',
    description:
      'Join our weekly interactive sessions where we build real AI applications together.',
    images: ['https://boundaryml.com/baml-og-background.png'],
    creator: '@boundaryml',
    site: '@boundaryml',
  },
};

export default async function PodcastPage() {
  const fetched = await fetchPodcastEpisodes();
  const podcastEpisodes = fetched.length > 0 ? fetched : fallbackEpisodes;
  return (
    <div
      className="mx-auto relative"
      style={{
        maxWidth: 1600,
        background: '#FBF7ED',
      }}
    >
      <Navbar />
      <main className="flex flex-col items-center justify-center min-h-screen w-full">
        <HeroSection />

        {/* Episodes List */}
        <section className="w-full px-4 sm:px-6">
          <div className="mx-auto max-w-6xl">
            <PodcastEpisodesGrid
              episodes={podcastEpisodes.filter((ep) => !ep.featured)}
            />
          </div>
        </section>

        {/* Subscribe CTA */}
        <section className="w-full px-4 sm:px-6 py-12 sm:py-20">
          <div className="mx-auto max-w-4xl text-center">
            <Headphones className="h-8 w-8 sm:h-12 sm:w-12 mx-auto mb-4 sm:mb-6 text-primary" />
            <h2 className="text-2xl sm:text-3xl font-bold mb-4">
              Never Miss an Episode
            </h2>
            <p className="text-base sm:text-lg text-muted-foreground mb-6 sm:mb-8 max-w-2xl mx-auto">
              Join our weekly sessions and learn how to build AI that actually
              works in production.
            </p>
            <div className="flex flex-col sm:flex-row flex-wrap gap-3 sm:gap-4 justify-center">
              {podcastPlatforms.map((platform) => (
                <Link
                  key={platform.name}
                  href={platform.url}
                  className="editorial-btn"
                >
                  <span>{platform.icon}</span>
                  Subscribe on {platform.name}
                </Link>
              ))}
            </div>
            <style>{`
              .editorial-btn {
                display: inline-flex;
                align-items: center;
                gap: 10px;
                padding: 12px 22px;
                border-radius: 8px;
                background: #ffffff;
                color: #1A1612;
                border: 1px solid #D9D3C4;
                font-size: 14px;
                font-weight: 500;
                letter-spacing: 0.01em;
                text-decoration: none;
                transition: background-color 200ms ease, border-color 200ms ease, color 200ms ease, transform 200ms ease;
              }
              .editorial-btn:hover {
                background: #FBF8F1;
                border-color: #6D28D9;
                color: #6D28D9;
                transform: translateY(-1px);
              }
              @media (max-width: 640px) {
                .editorial-btn { width: 100%; justify-content: center; }
              }
            `}</style>
          </div>
        </section>

        <FooterSection />
      </main>
    </div>
  );
}
