// biome-ignore-all lint/style/useFilenamingConvention: Keep the test beside the existing component path.
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { ValueRenderer } from './ValueRenderer';

describe('ValueRenderer', () => {
  it('renders JSON tree rows with each key beside its collection toggle', () => {
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
    expect(markup).toContain('&quot;self&quot;');
    expect(markup).toContain('&quot;collector&quot;');
    expect(markup).not.toContain('&quot;expansions&quot;');

    const rootToggle = markup.indexOf('aria-label="Collapse object"');
    const selfToggle = markup.indexOf(
      'aria-label="Collapse object"',
      rootToggle + 1,
    );
    const selfKey = markup.indexOf('&quot;self&quot;');
    const collectorToggle = markup.indexOf('aria-label="Expand object"');
    const collectorKey = markup.indexOf('&quot;collector&quot;');
    expect(selfToggle).toBeLessThan(selfKey);
    expect(selfKey).toBeLessThan(collectorToggle);
    expect(collectorToggle).toBeLessThan(collectorKey);
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

  it('uses custom renderers for typed values inside the JSON tree', () => {
    const markup = renderToStaticMarkup(
      <ValueRenderer
        customRenderers={{
          $media: () => <span data-testid="media-preview">image preview</span>,
        }}
        displayMode="expanded"
        value={{
          media: {
            $type: '$media',
            content_type: 'url',
            media_type: 'image',
            url: 'https://example.com/image.png',
          },
        }}
      />,
    );

    expect(markup).toContain('&quot;media&quot;');
    expect(markup).toContain('data-testid="media-preview"');
    expect(markup).toContain('image preview');
    expect(markup).not.toContain('&quot;$type&quot;');
    expect(markup.indexOf('&quot;media&quot;')).toBeLessThan(
      markup.indexOf('data-testid="media-preview"'),
    );
  });
});
