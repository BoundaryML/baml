import type { Metadata } from 'next';
import localFont from 'next/font/local';
import { SectionViews } from '@/components/track';
import ForceDark from '@/components/v2/force-dark';
import Adopt from '@/components/v4/adopt';
import Book from '@/components/v4/book';
import Footer from '@/components/v4/footer';
import Hero from '@/components/v4/hero';
import Language from '@/components/v4/language';
import Nav from '@/components/v4/nav';
import Statement from '@/components/v4/statement';
import Workflow from '@/components/v4/workflow';
import '../v2/v2.css';
import '../diagram-styles/styles.css';
import '../v3/v3.css';
import './v4.css';

// Geist Pixel is BAML's voice in the brand, used here for the headline.
const pixel = localFont({
  display: 'swap',
  src: '../fonts/GeistPixel-Square.otf',
  variable: '--font-geist-pixel',
});

// Version 4 of the homepage: it sells the whole stack, and every section leads to booking a call. Its
// copy is in copy/v4.md.
export const metadata: Metadata = {
  description:
    'BAML is one toolchain from the compiler to production. Build, test, trace, query, and alert on your code with tools that all read the same types.',
  robots: { follow: true, index: false },
  title: 'BAML | The unified software stack for agents',
};

export default function Home() {
  return (
    <div
      className={`v2 v3-page min-h-screen ${pixel.variable}`}
      data-theme="dark"
      id="top"
    >
      <ForceDark />
      <Nav />
      <main id="main">
        <Hero />
        <Statement />
        <Workflow />
        <Language />
        <Adopt />
        <Book />
      </main>
      <Footer />
      <SectionViews />
    </div>
  );
}
