import { Suspense } from 'react';
import { createMetadata } from '@/app/_lib/metadata';
import '../learn2/learn2.css';
import '../learn3/learn3.css';
import '../learn4/learn4.css';
import '../baml-intro/baml-intro.css';
import { Article } from '../baml-intro/_components/article';

export const metadata = createMetadata({
  description:
    "Why agents need a real programming language, plus BAML's tooling for agents and humans and how to adopt it.",
  ogTitle: 'Why agents need a new language',
  path: '/explore-legacy',
  timeline: true,
  title: 'Explore BAML',
});

export default function ExplorePage() {
  // The `deep` view renders Part 1 (AI workflows) onward, with the "On this
  // page" rail. The hero + design philosophy live on the homepage.
  return (
    <Suspense>
      <Article view="deep" />
    </Suspense>
  );
}
