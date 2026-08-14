import { createMetadata } from '@/app/_lib/metadata';

// `/pricing` is a client component, so its metadata lives in this server layout.
export const metadata = createMetadata({
  description:
    'Free and open source, runs entirely on your machine, never calls our servers. Cloud tools come later this year.',
  ogTitle: 'Free and open source',
  path: '/pricing',
  title: 'Pricing',
});

export default function PricingLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return children;
}
