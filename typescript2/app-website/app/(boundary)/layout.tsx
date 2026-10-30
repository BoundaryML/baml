import appleIcon from '@boundaryml/brand/logo/favicon/boundary/apple-touch-icon.png';
import favicon32 from '@boundaryml/brand/logo/favicon/boundary/favicon-32.png';
import type { Metadata } from 'next';
import localFont from 'next/font/local';
import { SITE_URL, TWITTER_HANDLE } from '@/app/_lib/metadata';
import { OG_SITE } from '@/components/boundary/page-parts';
import { THEME_KEY } from '@/components/boundary/theme';
import { AnalyticsProvider } from '@/context/analytics';
import './boundary.css';

// Root layout for the Boundary pages (the homepage and /case-studies). The BAML pages have their own
// root layout in app/(baml), so the two sites' styles and themes never load together; moving between
// them is a full page load.

// Every face is self-hosted from ./fonts (latin subset), loaded with next/font/local, so the
// build needs no network access to Google Fonts.

// Geist for all UI and body text.
const geist = localFont({
  display: 'swap',
  src: './fonts/Geist.woff2',
  variable: '--font-geist',
  weight: '100 900',
});

const jetbrainsMono = localFont({
  display: 'swap',
  src: './fonts/JetBrainsMono.woff2',
  variable: '--font-jetbrains-mono',
  weight: '100 800',
});

// Lexend is the display face (--font-display): every headline and section heading, including the
// .h2-display rule in boundary.css.
const lexend = localFont({
  display: 'swap',
  src: './fonts/Lexend.woff2',
  variable: '--font-lexend',
  weight: '100 900',
});

// Placeholder customer logos on the logo wall; each fake company gets its own face.
// next/font needs each loader call assigned to its own const
const sora = localFont({
  display: 'swap',
  src: './fonts/Sora.woff2',
  variable: '--font-sora',
  weight: '300 600',
});
const spaceGrotesk = localFont({
  display: 'swap',
  src: './fonts/SpaceGrotesk.woff2',
  variable: '--font-space-grotesk',
  weight: '500 700',
});
const fraunces = localFont({
  display: 'swap',
  src: './fonts/Fraunces.woff2',
  variable: '--font-fraunces',
  weight: '400 600',
});
const archivoBlack = localFont({
  display: 'swap',
  src: './fonts/ArchivoBlack.woff2',
  variable: '--font-archivo-black',
  weight: '400',
});
const manrope = localFont({
  display: 'swap',
  src: './fonts/Manrope.woff2',
  variable: '--font-manrope',
  weight: '600 800',
});
const dmSerif = localFont({
  display: 'swap',
  src: './fonts/DMSerifDisplay.woff2',
  variable: '--font-dm-serif',
  weight: '400',
});
const plex = localFont({
  display: 'swap',
  src: './fonts/IBMPlexSans-600.woff2',
  variable: '--font-plex',
  weight: '600',
});
const syne = localFont({
  display: 'swap',
  src: './fonts/Syne-800.woff2',
  variable: '--font-syne',
  weight: '800',
});
const logoFonts = [
  sora,
  spaceGrotesk,
  fraunces,
  archivoBlack,
  manrope,
  dmSerif,
  plex,
  syne,
];

// Geist Pixel (Square) is BAML's voice only (--font-pixel), not Boundary's: the BAML section and
// the footer strapline. Boundary's own headings stay on Lexend.
const geistPixel = localFont({
  display: 'swap',
  src: './fonts/GeistPixel-Square.otf',
  variable: '--font-geist-pixel',
});

const THEME_SCRIPT = `try{var t=localStorage.getItem(${JSON.stringify(THEME_KEY)});if(t!=="light"&&t!=="dark")t=matchMedia("(prefers-color-scheme: dark)").matches?"dark":"light";document.documentElement.dataset.theme=t}catch(e){}`;

const DESCRIPTION =
  'Query OTEL traces to create alerts, dashboards, AI evals, or build your own tooling. Write your AI workflows in the BAML language to record rich, cheap telemetry data built for agents.';

export const metadata: Metadata = {
  description: DESCRIPTION,
  // The brand package's Boundary favicons.
  icons: { apple: appleIcon.src, icon: favicon32.src },
  // Absolute base for the share image (OG_SITE), shared with the BAML pages (app/_lib/metadata.ts).
  metadataBase: new URL(SITE_URL),
  openGraph: {
    ...OG_SITE,
    description: DESCRIPTION,
    title: 'Boundary | Agentic Observability',
    type: 'website',
  },
  // Pages set a bare title ("Case studies"); the template adds the site. The homepage uses `default` as-is.
  title: {
    default: 'Boundary | Agentic Observability',
    template: '%s · Boundary',
  },
  // No twitter.title/description: Next falls back to the page's own title/description, which already
  // vary per route (see each page's own `metadata`/`generateMetadata`).
  twitter: {
    card: 'summary_large_image',
    images: OG_SITE.images,
    site: TWITTER_HANDLE,
  },
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html
      className={`${geist.variable} ${jetbrainsMono.variable} ${lexend.variable} ${geistPixel.variable} ${logoFonts.map((f) => f.variable).join(' ')} h-full antialiased`}
      // Next 16 stops resetting scroll-behavior around navigations unless this is set. boundary.css sets
      // html { scroll-behavior: smooth }, so this keeps new pages opening at the top (instead of gliding
      // there) while in-page anchors still glide. Next 15 does this by default; the attribute is kept so
      // an upgrade doesn't regress it.
      data-scroll-behavior="smooth"
      lang="en"
      suppressHydrationWarning
    >
      <head>
        {/* Pick the theme before first paint: a saved choice wins, otherwise follow the system. */}
        {/* biome-ignore lint/security/noDangerouslySetInnerHtml: THEME_SCRIPT is a constant, defined above */}
        <script dangerouslySetInnerHTML={{ __html: THEME_SCRIPT }} />
      </head>
      <body className="min-h-full flex flex-col">
        <AnalyticsProvider>{children}</AnalyticsProvider>
      </body>
    </html>
  );
}
