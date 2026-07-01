import type { Metadata } from 'next';
import { Suspense } from 'react';
import '../learn2/learn2.css';
import './learn3.css';
import { Deck } from './_deck/Deck';

export const metadata: Metadata = {
  title: 'BAML — a language for nondeterministic software',
  description:
    'Typed model calls, inferred errors, colorless concurrency, tests and traces in one toolchain.',
};

export default function Learn3Page() {
  // Server component (keeps `metadata`); the client Deck builds the slides so
  // the BAML editors/playground live entirely client-side.
  return (
    <Suspense fallback={<div className="l2-loading font-mono">loading…</div>}>
      <Deck />
    </Suspense>
  );
}
