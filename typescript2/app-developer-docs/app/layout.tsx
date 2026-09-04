import { GeistMono } from 'geist/font/mono';
import { GeistSans } from 'geist/font/sans';
import type { Metadata, Viewport } from 'next';

import { SiteFooter } from '@/components/site-footer';
import { SiteHeader } from '@/components/site-header';
import { ThemeProvider } from '@/components/theme-provider';
import { shouldIndexDeployment } from '@/lib/deployment';
import { siteConfig } from '@/lib/site-config';

import './globals.css';
import './typeset.css';

export const viewport: Viewport = {
  themeColor: [
    { color: '#ffffff', media: '(prefers-color-scheme: light)' },
    { color: '#0a0a0a', media: '(prefers-color-scheme: dark)' },
  ],
};

export const metadata: Metadata = {
  authors: [{ name: 'Boundary', url: 'https://boundaryml.com' }],
  creator: 'Boundary',
  description: siteConfig.description,
  icons: {
    apple: '/apple-icon',
    icon: '/icon.svg',
    shortcut: '/icon.svg',
  },
  keywords: [
    'BAML',
    'AI applications',
    'structured outputs',
    'LLM programming language',
    'Boundary',
  ],
  manifest: '/manifest.webmanifest',
  metadataBase: new URL(siteConfig.url),
  openGraph: {
    description: siteConfig.description,
    locale: 'en_US',
    siteName: siteConfig.name,
    title: siteConfig.name,
    type: 'website',
    url: siteConfig.url,
  },
  robots: {
    follow: shouldIndexDeployment(),
    index: shouldIndexDeployment(),
  },
  title: {
    default: siteConfig.name,
    template: `%s · ${siteConfig.name}`,
  },
  twitter: {
    card: 'summary_large_image',
    description: siteConfig.description,
    title: siteConfig.name,
  },
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html
      className={`${GeistSans.variable} ${GeistMono.variable}`}
      lang="en"
      suppressHydrationWarning
    >
      <body className="group/body min-h-screen overscroll-none font-sans antialiased [--footer-height:3.5rem] xl:[--footer-height:6rem]">
        <ThemeProvider>
          <div
            className="group/layout relative z-10 flex min-h-svh flex-col bg-background"
            data-slot="layout"
          >
            <SiteHeader />
            <main className="flex min-h-0 flex-1 flex-col">{children}</main>
            <SiteFooter />
          </div>
        </ThemeProvider>
      </body>
    </html>
  );
}
