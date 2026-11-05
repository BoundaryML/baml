import type { ReactNode } from 'react';
import Book from '@/components/home/book';
import Footer from '@/components/home/footer';
import Nav from '@/components/home/nav';
import { SectionViews } from '@/components/home/track';
import '../../styles/v3.css';
import '../../styles/home.css';

/** The frame every blog page shares with the homepage: its nav, closing band, and footer, on the same
 *  dark BAML-lane ground. The page content sits in the homepage's 1200px column (BLOG_WRAP). */
export function BlogShell({ children }: { children: ReactNode }) {
  return (
    <div className="v3-page v4-page min-h-screen flex flex-col">
      <Nav current="blog" />
      <main className="flex-1" id="main">
        {children}
        <Book />
      </main>
      <Footer />
      <SectionViews />
    </div>
  );
}

/** The homepage's content column, for the blog's own sections. */
export const BLOG_WRAP = 'v3-wrap mx-auto px-(--frame-pad)';
