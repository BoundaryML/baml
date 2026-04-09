import React from 'react';
import TourLayout from './_components/TourLayout';
import TourRunner from './_components/TourRunner';

export default function PromptTransparency() {
  return (
    <TourLayout
      currentSlug="prompt-transparency"
      title="See What the Model Sees"
      description="No more prompt guessing games"
    >
      <div style={{ padding: '1rem', borderBottom: '1px solid var(--ifm-color-emphasis-300)' }}>
        <h2 style={{ margin: 0 }}>No more prompt guessing games</h2>
        <p style={{ margin: '0.5rem 0 0', color: 'var(--ifm-color-emphasis-700)' }}>
          Ever wondered what prompt actually gets sent to the model? In most frameworks, it's hidden
          behind layers of abstraction. BAML shows you everything.
        </p>
        <p style={{ margin: '0.5rem 0 0', fontSize: '0.875rem', color: 'var(--ifm-color-emphasis-600)' }}>
          The <strong>Prompt Preview</strong> tab shows the exact text going to the model. Full transparency—no hidden system prompts, no mystery.
        </p>
      </div>
      <TourRunner exampleKey="prompt-transparency" />
    </TourLayout>
  );
}
