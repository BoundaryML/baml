import type { Metadata } from 'next';
import { Suspense } from 'react';
import '../learn2/learn2.css';
import '../learn3/learn3.css';
import './learn5.css';
import { Tour } from './_components/Tour';

export const metadata: Metadata = {
  description:
    'A single-page tour of BAML: typed LLM functions, inferred errors, colorless concurrency, tests and evals as code — with live editors throughout.',
  title: 'BAML — language tour',
};

export default function Learn5Page() {
  // Server component (keeps `metadata`); the client Tour holds the live
  // editors and playground. Suspense covers BamlCode's highlighter promise.
  return (
    <Suspense>
      <Tour />
    </Suspense>
  );
}
