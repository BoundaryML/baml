import type { Metadata } from 'next';
import WhatIsBamlPage from './what-is-baml/page';

// Homepage renders the /what-is-baml page (the explore redesign); that route
// now redirects here (next.config.mjs). The previous homepage is preserved at
// app/baml-intro (reachable at /baml-intro).
export const metadata: Metadata = {
  // description falls through to the layout's homeDescription (the exposé) so
  // the search snippet matches the social card.
  title: 'Basically a Made-up Language',
};

export default function Page() {
  return <WhatIsBamlPage />;
}
