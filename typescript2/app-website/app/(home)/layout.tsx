import appleIcon from '@boundaryml/brand/logo/favicon/boundary/apple-touch-icon.png';
import favicon32 from '@boundaryml/brand/logo/favicon/boundary/favicon-32.png';
import type { Metadata, Viewport } from 'next';
import { OG_SITE, SITE_URL, TWITTER_HANDLE } from '@/app/_lib/metadata';
import { SiteStructuredData } from '@/components/structured-data';
import { AnalyticsProvider } from '@/context/analytics';
import { FONT_VARIABLES } from './fonts';
import './globals.css';

// The site's root layout. Every page follows the brand package's BAML lane (Clover purple on a dark
// theme) and loads ./globals.css. Pages other than the homepage and the blog use PageShell
// (components/home/page-shell.tsx) for the homepage's nav and footer.

const TITLE = 'Boundary | The hyperobservable software stack';
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

/** The root layout: the document with the site's fonts, structured data, and analytics around every
 *  page. */
export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html
      className={`${FONT_VARIABLES} antialiased`}
      // These pages are dark only, so the theme is fixed here rather than chosen before paint.
      data-theme="dark"
      id="top"
      lang="en"
    >
      <body className="min-h-full flex flex-col">
        <SiteStructuredData />
        <AnalyticsProvider>{children}</AnalyticsProvider>
        {/* Page grid rails; drawn in globals.css (.grid-rails). Purely visual. */}
        <div aria-hidden className="grid-rails">
          <div />
        </div>
      </body>
    </html>
  );
}
