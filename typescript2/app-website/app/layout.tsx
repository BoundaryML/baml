import type { Metadata } from 'next';
import { ThemeProvider } from '@/components/theme-provider';
import { cn } from '@/lib/utils';
import './globals.css';

import { GeistMono } from 'geist/font/mono';
import { GeistSans } from 'geist/font/sans';
import { Caveat, Instrument_Serif } from 'next/font/google';

const instrumentSerif = Instrument_Serif({
  style: ['normal', 'italic'],
  subsets: ['latin'],
  variable: '--font-serif',
  weight: '400',
});

const caveat = Caveat({
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
export const metadata: Metadata = {
  alternates: {
    canonical: `${baseUrl}/`,
    types: {
      'text/plain': '/llms.txt',
    },
  },
  description:
    'BAML is a statically-typed, expression-oriented language with first-class LLM functions.',
  icons: {
    icon: '/favico.ico',
  },
  metadataBase: new URL(
    process.env.VERCEL_ENV === 'production'
      ? 'https://boundaryml.com'
      : 'http://localhost:3000',
  ),
  openGraph: {
    description:
      'BAML is a statically-typed, expression-oriented language with first-class LLM functions.',
    locale: 'en_US',
    siteName: 'BAML',
    title: 'BAML',
    type: 'website',
    url: 'https://boundaryml.com',
  },
  title: 'BAML',
  twitter: {
    card: 'summary_large_image',
    creator: '@boundaryml',
    description: 'Typed prompt boundaries for structured-output LLM work.',
    site: '@boundaryml',
    title: 'BAML | First-class LLM functions',
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
        <Suspense>
          <AnalyticsProvider>
            <ThemeProvider
              attribute="class"
              defaultTheme="light"
              enableSystem={false}
            >
              {props.children}
            </ThemeProvider>
          </AnalyticsProvider>
        </Suspense>
      </body>
    </html>
  );
}
