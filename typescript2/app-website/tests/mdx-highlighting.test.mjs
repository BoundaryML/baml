import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { compileMDX } from 'next-mdx-remote/rsc';
import { renderToStaticMarkup } from 'react-dom/server';
import { unified } from 'unified';
import rehypeHighlightCode from '../lib/mdx/rehype-highlight-code.ts';
import rehypePreserveCodeProps from '../lib/mdx/rehype-preserve-code-props.ts';
import remarkCodeMetadata from '../lib/mdx/remark-code-metadata.ts';

async function render(source, components = {}) {
  const { content } = await compileMDX({
    components,
    options: {
      mdxOptions: {
        rehypePlugins: [rehypePreserveCodeProps, rehypeHighlightCode],
        remarkPlugins: [remarkCodeMetadata],
      },
    },
    source,
  });
  return renderToStaticMarkup(content);
}

function text(node) {
  return node.type === 'text'
    ? node.value
    : (node.children ?? []).map(text).join('');
}

test('BAML fences emit colored tokens in server-rendered MDX', async () => {
  const html = await render(
    '```baml\nfunction main() -> int { let value = 42; value }\n```',
  );
  assert.match(html, /class="language-baml"/);
  assert.match(html, /<span style="color:#[A-Fa-f0-9]+">function<\/span>/);
  assert.match(html, /<span style="color:#[A-Fa-f0-9]+">\s*42<\/span>/);
  assert.ok(
    new Set(
      [...html.matchAll(/color:(#[A-Fa-f0-9]+)/g)].map((match) => match[1]),
    ).size >= 3,
  );
});

test('the actual 0.19.0 post highlights every BAML fence', async () => {
  const source = readFileSync(
    new URL('../blog-releases/2026-09-08-baml-0.19.0.md', import.meta.url),
    'utf8',
  );
  const fences = [...source.matchAll(/^```baml\n([\s\S]*?)^```/gm)];
  assert.ok(fences.length > 20);
  for (const [fence] of fences) {
    assert.match(await render(fence), /<span style="color:/);
  }
});

test('highlighting preserves code text, blank lines, and metadata', async () => {
  const source =
    // biome-ignore lint/suspicious/noTemplateCurlyInString: Preserve BAML interpolation literally.
    '// <tag> & "quotes"\n\nfunction main() -> string {\n    `hello ${42}`\n}\n';
  const code = {
    children: [{ type: 'text', value: source }],
    properties: { className: ['language-baml'] },
    tagName: 'code',
    type: 'element',
  };
  const pre = {
    children: [code],
    properties: { filename: 'example.baml' },
    tagName: 'pre',
    type: 'element',
  };
  await unified()
    .use(rehypeHighlightCode)
    .run({ children: [pre], type: 'root' });
  assert.equal(text(pre), source);
  assert.equal(pre.properties.filename, 'example.baml');
  assert.deepEqual(code.properties.className, ['language-baml']);
  assert.equal(pre.children[0], code);
});

test('tabbed examples retain direct pre children and filename labels', async () => {
  const html = await render(
    '<CodeBlocks>\n\n```baml filename="main.baml"\nclass Answer { value: int }\n```\n\n```python filename="main.py"\nprint(42)\n```\n\n</CodeBlocks>',
    {
      CodeBlocks: ({ children }) => {
        assert.equal(children.length, 2);
        assert.deepEqual(
          children.map((child) => child.type),
          ['pre', 'pre'],
        );
        assert.deepEqual(
          children.map((child) => child.props.filename),
          ['main.baml', 'main.py'],
        );
        return children;
      },
    },
  );
  assert.equal([...html.matchAll(/class="shiki github-dark"/g)].length, 2);
  assert.match(html, /class="language-python"/);
});

test('inline, unlabelled, and unsupported code remain plain text', async () => {
  const html = await render(
    '`inline`\n\n```\nplain\n```\n\n```unknown-language\n<foo>\n```',
  );
  assert.match(html, /<code>inline<\/code>/);
  assert.match(html, /<pre><code>plain\n<\/code><\/pre>/);
  assert.match(html, /&lt;foo&gt;/);
  assert.doesNotMatch(html, /shiki|<span/);
});
