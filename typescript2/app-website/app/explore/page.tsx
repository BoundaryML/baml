import type { Metadata } from 'next';
import { Suspense } from 'react';
import '../learn2/learn2.css';
import '../learn3/learn3.css';
import '../learn4/learn4.css';
import '../baml-intro/baml-intro.css';
import { Article } from '../baml-intro/_components/Article';

export const metadata: Metadata = {
  description:
    'BAML for AI workflows and agents: native LLM functions, tests, a sound type system, namespaces, baml describe, baml pack, green threads, and more.',
  title: 'Explore BAML',
};

export default function ExplorePage() {
  // The `deep` view renders Part 1 (AI workflows) onward, with the "On this
  // page" rail. The hero + design philosophy live on the homepage.
  return (
    <Suspense>
      <Article view="deep" />
    </Suspense>
  );
}
