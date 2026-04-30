import type { Metadata } from 'next';
import { redirect } from 'next/navigation';
export const metadata: Metadata = {
  description:
    'BAML is a statically-typed, expression-oriented language with first-class LLM functions.',
  title: 'BAML - First-class LLM functions',
};

export default function Home() {
  redirect('/');
}
