import React from 'react';
import TourLayout from './_components/TourLayout';
import TourRunner from './_components/TourRunner';

export default function PuttingItTogether() {
  return (
    <TourLayout
      currentSlug="putting-it-together"
      title="Build Something Real"
      description="You've learned the pieces. Now build with them."
    >
      <div style={{ padding: '1rem', borderBottom: '1px solid var(--ifm-color-emphasis-300)' }}>
        <h2 style={{ margin: 0 }}>You've learned the pieces. Now build with them.</h2>
        <p style={{ margin: '0.5rem 0 0', color: 'var(--ifm-color-emphasis-700)' }}>
          This module combines everything: types, unions, tests, streaming, and integration into a realistic feature—a Customer Support Triage Bot.
        </p>
        <p style={{ margin: '0.5rem 0 0', fontSize: '0.875rem', color: 'var(--ifm-color-emphasis-600)' }}>
          This is production-ready code. Real types, real tests, real reliability. Not a demo.
        </p>
      </div>
      <TourRunner exampleKey="putting-it-together" />
    </TourLayout>
  );
}
