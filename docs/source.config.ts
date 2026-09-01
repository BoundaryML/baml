import bamlGrammar from '../typescript2/pkg-grammar/baml.tmLanguage.json';
import { defineConfig, defineDocs } from 'fumadocs-mdx/config';
import { metaSchema, pageSchema } from 'fumadocs-core/source/schema';

export const docs = defineDocs({
  dir: 'content',
  docs: {
    schema: pageSchema,
  },
  meta: {
    schema: metaSchema,
  },
});

export default defineConfig({
  mdxOptions: {
    rehypeCodeOptions: {
      langs: [bamlGrammar],
      themes: {
        light: 'github-light',
        dark: 'github-dark',
      },
    },
  },
});
