import type { ReactNode } from 'react';
import Footer from './footer';
import Nav from './nav';
import { SectionViews } from './track';
import '@/app/(home)/styles/v3.css';
import '@/app/(home)/styles/home.css';

/** The frame for the site's other pages (the team page, the quickstart, the podcast, the legal
 *  pages, and so on): the homepage's nav and footer on the same dark BAML-lane ground. None of these
 *  pages is linked from the nav. The content column is PAGE_WRAP, the homepage's 1200px column. */
export function PageShell({ children }: { children: ReactNode }) {
  return (
    <div className="v3-page v4-page min-h-screen flex flex-col">
      <Nav />
      <main className="flex-1" id="main">
        {children}
      </main>
      <Footer />
      <SectionViews />
    </div>
  );
}

/** The homepage's content column. */
export const PAGE_WRAP = 'v3-wrap mx-auto px-(--frame-pad)';
