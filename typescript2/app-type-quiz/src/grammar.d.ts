// `@b/pkg-grammar-hljs` is published as plain JavaScript: it is a highlight.js
// language definition, and highlight.js's own registration signature is the
// only contract it has.
declare module '@b/pkg-grammar-hljs' {
  import type { LanguageFn } from 'highlight.js';
  const baml: LanguageFn;
  export default baml;
}
