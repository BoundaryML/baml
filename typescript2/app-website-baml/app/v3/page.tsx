import type { Metadata } from 'next';
import { Instrument_Serif } from 'next/font/google';
import localFont from 'next/font/local';
import { SectionViews } from '@/components/track';
import ForceDark from '@/components/v2/force-dark';
import Nav from '@/components/v2/nav';
import Hero from '@/components/v3/hero';
import '../v2/v2.css';
import '../diagram-styles/styles.css';
import './v3.css';

// Geist Pixel is BAML's voice in the brand, used here for the headline.
const pixel = localFont({
  display: 'swap',
  src: '../fonts/GeistPixel-Square.otf',
  variable: '--font-geist-pixel',
});

// The serif for the Ink style, in the hero version drawn with the /diagram-styles themes.
const serif = Instrument_Serif({
  display: 'swap',
  subsets: ['latin'],
  variable: '--font-instrument-serif',
  weight: '400',
});

// Version 3 of the homepage, in the /v2 branding. Only the hero exists so far, and the nav's section
// links still go to /v2. Its copy is in the v3 section of copy/home.md.
export const metadata: Metadata = {
  robots: { follow: true, index: false },
  title: 'BAML | The programming language for agents (v3)',
};

export default function Home() {
  return (
    <div
      className={`v2 v3-page min-h-screen ${pixel.variable} ${serif.variable}`}
      data-theme="dark"
    >
      <ForceDark />
      <Nav />
      <main id="main">
        <Hero />
      </main>
      <SectionViews />
    </div>
  );
}
