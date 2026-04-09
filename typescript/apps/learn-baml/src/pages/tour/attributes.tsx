import React from 'react';
import TourLayout from './_components/TourLayout';
import TourRunner from './_components/TourRunner';

export default function Attributes() {
  return (
    <TourLayout
      currentSlug="attributes"
      title="Attributes"
      description="@description, @alias, and @skip—fine-tune your schema"
    >
      <div style={{ padding: '1rem', borderBottom: '1px solid var(--ifm-color-emphasis-300)' }}>
        <h2 style={{ margin: 0 }}>Fine-tune how types become prompts</h2>
        <p style={{ margin: '0.5rem 0 0', color: 'var(--ifm-color-emphasis-700)' }}>
          Attributes give you precise control over how your schema translates to prompts.
          Add context with <code>@description</code>, rename fields with <code>@alias</code>, or hide sensitive fields with <code>@skip</code>.
        </p>
        <p style={{ margin: '0.5rem 0 0', fontSize: '0.875rem', color: 'var(--ifm-color-emphasis-600)' }}>
          Notice how <code>@skip</code> completely removes the SSN field from the prompt—the model never sees it exists.
        </p>
      </div>
      <TourRunner exampleKey="attributes" />
    </TourLayout>
  );
}
