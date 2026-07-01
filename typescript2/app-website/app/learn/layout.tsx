import { createMetadata } from '@/app/_lib/metadata';

// `/learn` is a client component, so its metadata lives in this server layout.
// `/learn/everything` overrides this with its own page-level metadata.
export const metadata = createMetadata({
  description:
    'Learn BAML interactively — from your first typed LLM function to production-ready agents.',
  eyebrow: 'Learn',
  ogTitle: 'Learn BAML',
  path: '/learn',
  title: 'Learn',
});

export default function LearnLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return children;
}
