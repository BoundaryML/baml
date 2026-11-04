import appleIcon from '@boundaryml/brand/logo/favicon/boundary/apple-touch-icon.png';
import favicon32 from '@boundaryml/brand/logo/favicon/boundary/favicon-32.png';
import type { Metadata, Viewport } from 'next';
import localFont from 'next/font/local';
import { OG_SITE, SITE_URL, TWITTER_HANDLE } from '@/app/_lib/metadata';
import { AnalyticsProvider } from '@/context/analytics';
import './globals.css';

// Root layout for the homepage and the blog. They follow the brand package's BAML lane (Clover purple
// on a dark theme) and load ./globals.css. The BAML pages (app/(baml)) have their own root layout, so
// moving between the two groups is a full page load.

// Every face is self-hosted from ./fonts, so the build needs no network access.
const geist = localFont({
  display: 'swap',
  src: './fonts/Geist.woff2',
  variable: '--font-geist',
  weight: '100 900',
});
const lexend = localFont({
  display: 'swap',
  src: './fonts/Lexend.woff2',
  variable: '--font-lexend',
  weight: '100 900',
});
const jetbrainsMono = localFont({
  display: 'swap',
  src: './fonts/JetBrainsMono.woff2',
  variable: '--font-jetbrains-mono',
  weight: '100 800',
});
// Geist Pixel Square, the brand's caption face.
const geistPixel = localFont({
  display: 'swap',
  src: './fonts/GeistPixel-Square.otf',
  variable: '--font-geist-pixel',
});

const TITLE = 'Boundary | The unified software stack for agents';
const DESCRIPTION =
  'BAML is one toolchain from the compiler to production. Build, test, trace, query, and alert on your code with tools that all read the same types.';

export const metadata: Metadata = {
  description: DESCRIPTION,
  // The brand package's Boundary favicons.
  icons: { apple: appleIcon.src, icon: favicon32.src },
  metadataBase: new URL(SITE_URL),
  openGraph: {
    ...OG_SITE,
    description: DESCRIPTION,
    title: TITLE,
    type: 'website',
  },
  // Pages set a bare title ("Blog"); the template adds the site. The homepage uses `default` as-is.
  title: { default: TITLE, template: '%s · Boundary' },
  twitter: {
    card: 'summary_large_image',
    images: OG_SITE.images,
    site: TWITTER_HANDLE,
  },
};

export const viewport: Viewport = {
  colorScheme: 'dark',
  themeColor: '#0a0a0a',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html
      className={`${geist.variable} ${lexend.variable} ${jetbrainsMono.variable} ${geistPixel.variable} antialiased`}
      // These pages are dark only, so the theme is fixed here rather than chosen before paint.
      data-theme="dark"
      id="top"
      lang="en"
    >
      <body className="min-h-full flex flex-col">
        <AnalyticsProvider>{children}</AnalyticsProvider>
        {/* Page grid rails; drawn in globals.css (.grid-rails). Purely visual. */}
        <div aria-hidden className="grid-rails">
          <div />
        </div>
      </body>
    </html>
  );
}
