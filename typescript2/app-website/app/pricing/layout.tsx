import { createMetadata } from '@/app/_lib/metadata';

// `/pricing` is a client component, so its metadata lives in this server layout.
export const metadata = createMetadata({
  description:
    'Simple pricing for BAML and Boundary — start free, scale to production, with enterprise support available.',
  eyebrow: 'Pricing',
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
