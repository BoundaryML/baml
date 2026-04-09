import React from 'react';
import TourLayout from './_components/TourLayout';
import TourRunner from './_components/TourRunner';

export default function UnionTypes() {
  return (
    <TourLayout
      currentSlug="union-types"
      title="Unions for Messy Reality"
      description="Handle multiple outcomes with type safety"
    >
      <div style={{ padding: '1rem', borderBottom: '1px solid var(--ifm-color-emphasis-300)' }}>
        <h2 style={{ margin: 0 }}>Handle multiple outcomes with type safety</h2>
        <p style={{ margin: '0.5rem 0 0', color: 'var(--ifm-color-emphasis-700)' }}>
          LLMs don't always give you what you expect. Sometimes they need clarification.
          Sometimes they refuse. BAML lets you handle all outcomes in one function.
        </p>
        <p style={{ margin: '0.5rem 0 0', fontSize: '0.875rem', color: 'var(--ifm-color-emphasis-600)' }}>
          One function, multiple typed outcomes. No more checking <code>if response.error</code> or parsing edge cases manually.
        </p>
      </div>
      <TourRunner exampleKey="union-types" />
    </TourLayout>
  );
}
