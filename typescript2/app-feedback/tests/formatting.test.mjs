import { expect, test } from "bun:test";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { FormattedText } from "../src/components/code.tsx";

test("Markdown renders paragraphs, inline code, headings, lists and BAML coloring", () => {
  const html = renderToStaticMarkup(React.createElement(FormattedText, {text:'## Repro\n\nFirst `baml check`.\n\n**Expected:** success\n\n- Step one\n- Step two\n\n```baml\nclass Item { name string }\n```'}));
  for (const marker of ['<h3', '<p', '<code', '<strong>Expected:</strong>', '<ul', '<li', 'text-purple-', 'text-blue-']) expect(html).toContain(marker);
  expect(html).not.toContain('```');
});
test("untrusted Markdown cannot run HTML, JavaScript links or tracking images", () => {
  const html = renderToStaticMarkup(React.createElement(FormattedText, {text:'<script>alert(1)</script>\n\n[click](javascript:alert%281%29)\n\n![private](https://example.invalid/tracker)\n\n```baml\n"<script>code</script>"\n```'}));
  expect(html).not.toContain('<script>');
  expect(html).not.toContain('href="javascript:');
  expect(html).not.toContain('<img');
  expect(html).toContain('&lt;script&gt;code&lt;/script&gt;');
});
