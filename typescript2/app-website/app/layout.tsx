import type { Metadata } from 'next';
import { ogImagePath, TWITTER_HANDLE } from '@/app/_lib/metadata';
import { SiteBanner } from '@/components/site-banner';
import { SiteStructuredData } from '@/components/structured-data';
import { ThemeProvider } from '@/components/theme-provider';
import { Whiteboard } from '@/components/whiteboard/whiteboard';
import { cn } from '@/lib/utils';
import './globals.css';

import { GeistMono } from 'geist/font/mono';
import { GeistSans } from 'geist/font/sans';
import { Caveat, Instrument_Serif } from 'next/font/google';

// Accent fonts: applied globally as CSS variables but only used on a handful of
// pages (serif on blog/podcast/jobs/fundraiser, caveat nowhere yet). Preloading
// them on every page emits unused `<link rel=preload>` tags — hence the browser
// "preloaded but not used within a few seconds" warning on e.g. /learn2. Let
// them load on demand (still swap-rendered) instead of preloading globally.
const instrumentSerif = Instrument_Serif({
  preload: false,
  style: ['normal', 'italic'],
  subsets: ['latin'],
  variable: '--font-serif',
  weight: '400',
});

const caveat = Caveat({
  preload: false,
  subsets: ['latin'],
  variable: '--font-caveat',
  weight: ['400', '500'],
});

import type { Viewport } from 'next';
import { Suspense } from 'react';
import { AnalyticsProvider } from '@/context/analytics';

const baseUrl =
  process.env.NEXT_PUBLIC_BASE_URL ??
  (process.env.VERCEL_PROJECT_PRODUCTION_URL
    ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
    : 'http://localhost:3000');
const homeDescription =
  'BAML is a programming language for agents. Python and TypeScript were built for human productivity. BAML was designed with a different goal in mind.';
// The homepage card shows the eras timeline with its own short subtitle; the
// meta description above does the selling in search results and link unfurls.
const homeOgImage = ogImagePath({
  description: 'New paradigm, new language.',
  timeline: true,
  title: 'The programming language for agents',
});

export const metadata: Metadata = {
  alternates: {
    canonical: `${baseUrl}/`,
  },
  description: homeDescription,
  icons: {
    icon: '/favico.ico',
  },
  metadataBase: new URL(baseUrl),
  openGraph: {
    description: homeDescription,
    images: [
      {
        alt: 'BAML: the programming language for agents',
        height: 630,
        url: homeOgImage,
        width: 1200,
      },
    ],
    locale: 'en_US',
    siteName: 'BAML',
    title: 'BAML: the programming language for agents',
    type: 'website',
    url: baseUrl,
  },
  title: {
    default: 'BAML: the programming language for agents',
    template: '%s | BAML',
  },
  twitter: {
    card: 'summary_large_image',
    creator: TWITTER_HANDLE,
    description: homeDescription,
    images: [homeOgImage],
    site: TWITTER_HANDLE,
    title: 'BAML: the programming language for agents',
  },
};

export const viewport: Viewport = {
  themeColor: [
    { color: 'white', media: '(prefers-color-scheme: light)' },
    { color: 'black', media: '(prefers-color-scheme: dark)' },
  ],
};

export default function RootLayout(props: { children: React.ReactNode }) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body
        className={cn(
          'bg-background text-foreground relative min-h-screen font-sans antialiased',
          GeistSans.variable,
          GeistMono.variable,
          instrumentSerif.variable,
          caveat.variable,
        )}
      >
        <SiteStructuredData />
        <Suspense>
          <AnalyticsProvider>
            <ThemeProvider
              attribute="class"
              defaultTheme="light"
              enableSystem={false}
            >
              <SiteBanner />
              {props.children}
              <Whiteboard />
            </ThemeProvider>
          </AnalyticsProvider>
        </Suspense>
      </body>
    </html>
  );
}
