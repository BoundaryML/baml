import React from 'react';
import TourLayout from './_components/TourLayout';
import TourRunner from './_components/TourRunner';

export default function ClientStrategies() {
  return (
    <TourLayout
      currentSlug="client-strategies"
      title="Retry and Fallback"
      description="Reliability is a configuration, not code"
    >
      <div style={{ padding: '1rem', borderBottom: '1px solid var(--ifm-color-emphasis-300)' }}>
        <h2 style={{ margin: 0 }}>Reliability is a configuration, not code</h2>
        <p style={{ margin: '0.5rem 0 0', color: 'var(--ifm-color-emphasis-700)' }}>
          Models fail. Rate limits hit. BAML handles this declaratively—no try/catch spaghetti.
        </p>
        <p style={{ margin: '0.5rem 0 0', fontSize: '0.875rem', color: 'var(--ifm-color-emphasis-600)' }}>
          Define retry policies and fallback chains. Swap models without touching your functions.
        </p>
      </div>
      <TourRunner exampleKey="client-strategies" />
    </TourLayout>
  );
}
