import React from 'react';
import TourLayout from './_components/TourLayout';
import TourRunner from './_components/TourRunner';

export default function PolyglotIntegration() {
  return (
    <TourLayout
      currentSlug="polyglot-integration"
      title="Call from Your Language"
      description="BAML fits into your stack"
    >
      <div style={{ padding: '1rem', borderBottom: '1px solid var(--ifm-color-emphasis-300)' }}>
        <h2 style={{ margin: 0 }}>BAML fits into your stack—you don't rewrite for BAML</h2>
        <p style={{ margin: '0.5rem 0 0', color: 'var(--ifm-color-emphasis-700)' }}>
          This is the "incrementally adoptable" promise. See BAML called from TypeScript, Python, and more.
        </p>
        <p style={{ margin: '0.5rem 0 0', fontSize: '0.875rem', color: 'var(--ifm-color-emphasis-600)' }}>
          Write once in BAML. Call from anywhere. Types flow through with IDE autocomplete in every language.
        </p>
      </div>
      <TourRunner exampleKey="polyglot-integration" />
    </TourLayout>
  );
}
