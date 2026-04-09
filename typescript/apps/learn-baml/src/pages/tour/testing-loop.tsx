import React from 'react';
import TourLayout from './_components/TourLayout';
import TourRunner from './_components/TourRunner';

export default function TestingLoop() {
  return (
    <TourLayout
      currentSlug="testing-loop"
      title="Tests That Run"
      description="Prompts deserve tests too"
    >
      <div style={{ padding: '1rem', borderBottom: '1px solid var(--ifm-color-emphasis-300)' }}>
        <h2 style={{ margin: 0 }}>Prompts deserve tests too</h2>
        <p style={{ margin: '0.5rem 0 0', color: 'var(--ifm-color-emphasis-700)' }}>
          Would you ship code without tests? Then why ship prompts without tests?
          BAML makes prompt testing as natural as unit testing.
        </p>
        <p style={{ margin: '0.5rem 0 0', fontSize: '0.875rem', color: 'var(--ifm-color-emphasis-600)' }}>
          Define test cases with <code>test</code> blocks and <code>@@assert</code>.
          CI/CD for prompts—catch regressions before they hit production.
        </p>
      </div>
      <TourRunner exampleKey="testing-loop" />
    </TourLayout>
  );
}
