import type { ReactNode } from 'react';

/**
 * Wraps a page's <main>. On Next 16 this is React's <ViewTransition>, which cross-fades the content on
 * a client-side navigation while the nav stays put. The React that Next 15 bundles only ships it
 * behind `experimental.viewTransition`, which moves the whole app to React's experimental channel, so
 * for now this renders its children unchanged. Full page loads still get the browser's cross-document
 * fade (`@view-transition` in app/(boundary)/boundary.css), and the page-enter/page-exit classes stay
 * there for when the site moves to Next 16.
 *
 * `id` keys the boundary per route; pages keep passing it so switching back touches only this file.
 */
export default function PageTransition({
  children,
}: {
  id: string;
  children: ReactNode;
}) {
  return children;
}
