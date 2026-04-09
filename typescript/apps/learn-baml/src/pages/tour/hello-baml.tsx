import React from 'react';
import TourLayout from './_components/TourLayout';
import TourRunner from './_components/TourRunner';

export default function HelloBaml() {
  return (
    <TourLayout
      currentSlug="hello-baml"
      title="Your First BAML Function"
      description="Write an LLM function in 10 lines"
    >
      <div style={{ padding: '1rem', borderBottom: '1px solid var(--ifm-color-emphasis-300)' }}>
        <h2 style={{ margin: 0 }}>Write an LLM function in 10 lines</h2>
        <p style={{ margin: '0.5rem 0 0', color: 'var(--ifm-color-emphasis-700)' }}>
          Ever had an LLM return malformed JSON? Or get a string when you wanted a structured object?
          BAML makes that impossible. Click <strong>Run</strong> to see the prompt preview and output.
        </p>
        <p style={{ margin: '0.5rem 0 0', fontSize: '0.875rem', color: 'var(--ifm-color-emphasis-600)' }}>
          Notice how <code>{'{{ ctx.output_format }}'}</code> expands into schema instructions, and the output is a typed enum value—not a string to parse.
        </p>
      </div>
      <TourRunner exampleKey="hello-baml" />
    </TourLayout>
  );
}
