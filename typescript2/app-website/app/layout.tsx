import type { Metadata } from 'next';
import { ThemeProvider } from '@/components/theme-provider';
import { cn } from '@/lib/utils';
import './globals.css';

import { GeistMono } from 'geist/font/mono';
import { GeistSans } from 'geist/font/sans';
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
  },
  description:
    'BAML is the first language designed with LLMs in mind. Cognitive coding for AI—code that agents write, software that humans trust.',
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
      'BAML is the first language designed with LLMs in mind. Cognitive coding for AI—code that agents write, software that humans trust.',
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
    description:
      'Code that agents write. Software that humans trust. The first language designed with LLMs in mind.',
    site: '@boundaryml',
    title: 'BAML | The language for AI in the future',
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
