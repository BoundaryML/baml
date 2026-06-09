import type { Metadata } from 'next';
import { Suspense } from 'react';
import { Deck } from './_deck/Deck';
import './learn2.css';

export const metadata: Metadata = {
  title: 'Learn BAML',
  description: 'A guided, slide-by-slide tour of BAML.',
};

export default function Learn2Page() {
  // Server component (keeps `metadata`); the client Deck builds the slides so
  // the BAML editors/playground live entirely client-side.
  return (
    <Suspense fallback={<div className="l2-loading font-mono">loading…</div>}>
      <Deck />
    </Suspense>
  );
}
