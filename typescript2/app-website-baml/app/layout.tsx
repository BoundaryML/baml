import appleIcon from '@boundaryml/brand/logo/favicon/baml/apple-touch-icon.png';
import favicon32 from '@boundaryml/brand/logo/favicon/baml/favicon-32.png';
import type { Metadata, Viewport } from 'next';
import localFont from 'next/font/local';
import { SITE_URL } from '@/lib/site';
import './globals.css';

// All faces are self-hosted from app/fonts, so builds never hit the network.
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

// Turbopack hands an image imported from a package back as its URL, where webpack gives Next's
// { src, width, height } object; take the URL either way.
const url = (image: string | { src: string }) =>
  typeof image === 'string' ? image : image.src;

export const metadata: Metadata = {
  description:
    'BAML is a Turing-complete language for software written by agents and humans: TypeScript-like syntax, types that hold at runtime, explicit errors.',
  // The brand package's BAML favicons.
  icons: { apple: url(appleIcon), icon: url(favicon32) },
  metadataBase: new URL(SITE_URL),
  openGraph: {
    description:
      "Modern programming languages weren't built for agents. BAML is open source, pre-1.0, and usable today.",
    title: 'BAML — the programming language for agents',
    type: 'website',
  },
  title: 'BAML — the programming language for agents',
  twitter: { card: 'summary_large_image' },
};

export const viewport: Viewport = {
  colorScheme: 'dark light',
  themeColor: [
    { color: '#0a0a0a', media: '(prefers-color-scheme: dark)' },
    { color: '#ffffff', media: '(prefers-color-scheme: light)' },
  ],
};

/* Runs before first paint, so a remembered choice never flashes the other theme. Nothing is written
   to <html> unless the visitor has actually chosen: with no choice, the stylesheet's
   prefers-color-scheme block decides, which also keeps the server and client markup identical. */
const NO_FLASH = `try{var t=localStorage.getItem("baml-theme");if(t==="light"||t==="dark")document.documentElement.dataset.theme=t}catch(e){}`;

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html
      className={`${geist.variable} ${lexend.variable} ${jetbrainsMono.variable} antialiased`}
      id="top"
      lang="en"
      suppressHydrationWarning
    >
      <head>
        {/* biome-ignore lint/security/noDangerouslySetInnerHtml: NO_FLASH is a constant, defined above */}
        <script dangerouslySetInnerHTML={{ __html: NO_FLASH }} />
      </head>
      <body className="min-h-full flex flex-col">
        {children}
        {/* Page grid rails; drawn in globals.css (.grid-rails). Purely visual. */}
        <div aria-hidden className="grid-rails">
          <div />
        </div>
      </body>
    </html>
  );
}
