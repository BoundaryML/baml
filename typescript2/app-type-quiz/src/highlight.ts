// Highlighting, from the grammar and nothing else.
//
// This is deliberately lexical: the highlighter is told what the tokens are,
// never what they mean. A semantic highlighter would have to resolve the
// program, and an unresolved name or a mismatched type showing up in a colour
// would answer the question the quiz is asking.

import baml from '@b/pkg-grammar-hljs';
import hljs from 'highlight.js/lib/core';

hljs.registerLanguage('baml', baml);

/** `source` as HTML, with token spans. */
export function highlight(source: string): string {
  return hljs.highlight(source, { ignoreIllegals: true, language: 'baml' })
    .value;
}
