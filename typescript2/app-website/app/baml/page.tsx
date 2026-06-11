import { redirect } from 'next/navigation';
import { createMetadata } from '@/app/_lib/metadata';

export const metadata = createMetadata({
  description:
    'BAML is a statically-typed, expression-oriented language with first-class LLM functions.',
  eyebrow: 'The Language',
  ogTitle: 'BAML — First-class LLM functions',
  path: '/baml',
  title: 'First-class LLM functions',
  titleAbsolute: 'BAML — First-class LLM functions',
});

export default function Home() {
  redirect('/');
}
