import React from 'react';
import TourLayout from './_components/TourLayout';
import TourRunner from './_components/TourRunner';

export default function TypesAtWork() {
  return (
    <TourLayout
      currentSlug="types-at-work"
      title="Types That Do Work"
      description="Your schema is your prompt (and your parser)"
    >
      <div style={{ padding: '1rem', borderBottom: '1px solid var(--ifm-color-emphasis-300)' }}>
        <h2 style={{ margin: 0 }}>Your schema is your prompt (and your parser)</h2>
        <p style={{ margin: '0.5rem 0 0', color: 'var(--ifm-color-emphasis-700)' }}>
          "Schema engineering {'>'} prompt engineering" — this is BAML's key insight.
          The types you define drive the prompt AND the parsing.
        </p>
        <p style={{ margin: '0.5rem 0 0', fontSize: '0.875rem', color: 'var(--ifm-color-emphasis-600)' }}>
          The <code>?</code> on fields means "optional"—and BAML tells the model this.
          See how <code>{'{{ ctx.output_format }}'}</code> expands into detailed schema instructions.
        </p>
      </div>
      <TourRunner exampleKey="types-at-work" />
    </TourLayout>
  );
}
