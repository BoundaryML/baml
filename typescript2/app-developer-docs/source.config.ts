import { defineConfig, defineDocs } from 'fumadocs-mdx/config';
import {
  authoredPageSchema,
  bookPageSchema,
} from './lib/content/book-perspective-schema';

export const docs = defineDocs({
  dir: 'content',
  docs: {
    files: ['**/*.mdx', '!baml/book/**'],
    schema: authoredPageSchema,
  },
});

export const book = defineDocs({
  dir: 'content/baml/book',
  docs: { files: ['**/*.mdx'], schema: bookPageSchema },
});

export default defineConfig();
