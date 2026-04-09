import React from 'react';
import TourLayout from './_components/TourLayout';
import TourRunner from './_components/TourRunner';

export default function DynamicTypes() {
  return (
    <TourLayout
      currentSlug="dynamic-types"
      title="Dynamic Types"
      description="Add fields at runtime with TypeBuilder"
    >
      <div style={{ padding: '1rem', borderBottom: '1px solid var(--ifm-color-emphasis-300)' }}>
        <h2 style={{ margin: 0 }}>Schemas that adapt at runtime</h2>
        <p style={{ margin: '0.5rem 0 0', color: 'var(--ifm-color-emphasis-700)' }}>
          Sometimes you don't know your schema until runtime—user-configured forms, database-driven fields, or plugin systems.
          <code>@@dynamic</code> and <code>TypeBuilder</code> let you add properties on the fly.
        </p>
        <p style={{ margin: '0.5rem 0 0', fontSize: '0.875rem', color: 'var(--ifm-color-emphasis-600)' }}>
          Mark a class with <code>@@dynamic</code> in BAML, then use <code>TypeBuilder</code> in your code to add fields before calling the function.
        </p>
      </div>
      <TourRunner exampleKey="dynamic-types" />
    </TourLayout>
  );
}
