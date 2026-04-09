import React from 'react';
import TourLayout from './_components/TourLayout';
import TourRunner from './_components/TourRunner';

export default function MatchExpressions() {
  return (
    <TourLayout
      currentSlug="match-expressions"
      title="Match for Control Flow"
      description="Handle every outcome explicitly"
    >
      <div style={{ padding: '1rem', borderBottom: '1px solid var(--ifm-color-emphasis-300)' }}>
        <h2 style={{ margin: 0 }}>Handle every outcome explicitly</h2>
        <p style={{ margin: '0.5rem 0 0', color: 'var(--ifm-color-emphasis-700)' }}>
          When your function returns a union, you need to handle each case.
          <code>match</code> forces you to handle them all—the compiler won't let you forget.
        </p>
        <p style={{ margin: '0.5rem 0 0', fontSize: '0.875rem', color: 'var(--ifm-color-emphasis-600)' }}>
          Exhaustiveness checking ensures you handle every case. Remove a case and watch the compiler tell you.
        </p>
      </div>
      <TourRunner exampleKey="match-expressions" />
    </TourLayout>
  );
}
