import { createMetadata } from '@/app/_lib/metadata';

// The page's metadata lives in this server layout.
export const metadata = createMetadata({
  description:
    'BAML is free and open source. Boundary Cloud has Starter at $0 a month, Pro at $250 a month and Enterprise, all with unlimited spans.',
  ogTitle: 'Pricing',
  path: '/pricing',
  title: 'Pricing',
});

/** Renders the pricing page unchanged. The layout exists only to carry the page's metadata. */
export default function PricingLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return children;
}
