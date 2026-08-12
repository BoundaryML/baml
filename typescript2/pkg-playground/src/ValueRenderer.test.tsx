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
    expect(markup).toContain(
      'aria-expanded="true" aria-label="Collapse object"',
    );
    expect(markup).toContain(
      'aria-expanded="true" aria-label="Collapse object self"',
    );
    expect(markup).toContain(
      'aria-expanded="false" aria-label="Expand object collector"',
    );
    expect(markup).toContain('&quot;self&quot;');
    expect(markup).toContain('&quot;collector&quot;');
    expect(markup).not.toContain('&quot;expansions&quot;');

    const selfToggle = markup.indexOf('aria-label="Collapse object self"');
    const selfKey = markup.indexOf('&quot;self&quot;');
    const collectorToggle = markup.indexOf(
      'aria-label="Expand object collector"',
    );
    const collectorKey = markup.indexOf('&quot;collector&quot;');
    expect(selfToggle).toBeLessThan(selfKey);
    expect(selfKey).toBeLessThan(collectorToggle);
    expect(collectorToggle).toBeLessThan(collectorKey);
  });

  it('identifies initially collapsed nested collection toggles', () => {
    const markup = renderToStaticMarkup(
      <ValueRenderer
        displayMode="expanded"
        value={{
          outer: {
            arrayChild: [1, 2],
            objectChild: { value: true },
          },
        }}
      />,
    );

    expect(markup).toContain(
      'aria-expanded="false" aria-label="Expand array arrayChild"',
    );
    expect(markup).toContain(
      'aria-expanded="false" aria-label="Expand object objectChild"',
    );
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

  it('hides unregistered type metadata in tree and inline modes', () => {
    const value = { $type: '$unknown', visible: 'value' };
    const treeMarkup = renderToStaticMarkup(
      <ValueRenderer displayMode="expanded" value={value} />,
    );
    const inlineMarkup = renderToStaticMarkup(
      <ValueRenderer displayMode="inline" value={value} />,
    );

    expect(treeMarkup).toContain('&quot;visible&quot;');
    expect(inlineMarkup).toContain('&quot;visible&quot;');
    expect(treeMarkup).not.toContain('&quot;$type&quot;');
    expect(inlineMarkup).not.toContain('&quot;$type&quot;');
  });

  it('bounds deeply nested inline collections', () => {
    const markup = renderToStaticMarkup(
      <ValueRenderer
        displayMode="inline"
        value={{ outer: { nested: { value: true } } }}
      />,
    );

    expect(markup).toContain('{…}');
    expect(markup).not.toContain('&quot;value&quot;');
  });
});
