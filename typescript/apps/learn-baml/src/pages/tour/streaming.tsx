import React from 'react';
import TourLayout from './_components/TourLayout';
import TourRunner from './_components/TourRunner';

export default function Streaming() {
  return (
    <TourLayout
      currentSlug="streaming"
      title="Streaming with Types"
      description="Stream structured data, not just text"
    >
      <div style={{ padding: '1rem', borderBottom: '1px solid var(--ifm-color-emphasis-300)' }}>
        <h2 style={{ margin: 0 }}>Stream structured data, not just text</h2>
        <p style={{ margin: '0.5rem 0 0', color: 'var(--ifm-color-emphasis-700)' }}>
          Streaming text is easy. Streaming <em>typed</em> data—where you can access fields as they arrive—that's the BAML difference.
        </p>
        <p style={{ margin: '0.5rem 0 0', fontSize: '0.875rem', color: 'var(--ifm-color-emphasis-600)' }}>
          Access <code>partial.title</code> before the full response is done. Build real-time UIs that show fields as they're ready.
        </p>
      </div>
      <TourRunner exampleKey="streaming" />
    </TourLayout>
  );
}
