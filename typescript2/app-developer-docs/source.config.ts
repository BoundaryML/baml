import { pageSchema } from 'fumadocs-core/source/schema';
import { defineConfig, defineDocs } from 'fumadocs-mdx/config';
import { z } from 'zod';

const breadcrumbSchema = z
  .object({
    href: z.string().startsWith('/').optional(),
    label: z.string().min(1),
  })
  .strict();

const authoredPageSchema = pageSchema
  .extend({
    breadcrumbs: z.array(breadcrumbSchema).min(1),
    description: z.string().min(1),
  })
  .strict();

export const docs = defineDocs({
  dir: 'content',
  docs: {
    files: ['**/*.mdx'],
    schema: authoredPageSchema,
  },
});

export default defineConfig();
