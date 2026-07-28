import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ValueRenderer } from './ValueRenderer';

describe('ValueRenderer', () => {
  it('renders nested objects as compact expandable rows without key counts', () => {
    const markup = renderToStaticMarkup(
      <ValueRenderer
        displayMode="expanded"
        value={{
          self: {
            collector: {
              expansions: {},
              loading_time_ms: 0,
              nested: { value: true },
            },
            result: 'ok',
          },
        }}
      />,
    );

    expect(markup).not.toMatch(/\d+ keys?/);
    expect(markup).toContain('aria-label="Collapse object"');
    expect(markup).toContain('aria-label="Expand object"');
    expect(markup).toContain('self:');
    expect(markup).toContain('collector:');
    expect(markup).not.toContain('expansions:');
  });

  it('keeps small nested collections expandable when initially collapsed', () => {
    const objectMarkup = renderToStaticMarkup(
      <ValueRenderer
        displayMode="expanded"
        value={{ outer: { inner: { values: [1, 2] } } }}
      />,
    );
    const arrayMarkup = renderToStaticMarkup(
      <ValueRenderer depth={2} displayMode="expanded" value={[1, 2]} />,
    );

    expect(objectMarkup).toContain('aria-label="Expand object"');
    expect(arrayMarkup).toContain('aria-label="Expand array"');
  });
});
