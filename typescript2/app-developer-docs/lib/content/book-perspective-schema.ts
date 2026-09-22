import { pageSchema } from 'fumadocs-core/source/schema';
import { z } from 'zod';

export const sectionKeysSchema = z.record(
  z.string().regex(/^[a-z][a-z0-9-]*$/),
  z
    .string()
    .min(1)
    .regex(/^[^#\s]+$/),
);

const breadcrumbsSchema = z
  .array(
    z
      .object({
        href: z.string().startsWith('/').optional(),
        label: z.string().min(1),
      })
      .strict(),
  )
  .min(1);

export const authoredPageSchema = pageSchema
  .extend({
    breadcrumbs: breadcrumbsSchema,
    description: z.string().min(1),
    sectionKeys: sectionKeysSchema.optional(),
  })
  .strict();

// Shared chapters and default.mdx require the full authored page metadata.
// Alternate versions can inherit description and breadcrumbs from default.mdx.
export const bookPageSchema = authoredPageSchema.partial({
  breadcrumbs: true,
  description: true,
});
