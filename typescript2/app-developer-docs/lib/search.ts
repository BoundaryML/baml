import { z } from 'zod';

const searchEntrySchema = z
  .object({
    current: z.boolean(),
    group: z.string(),
    href: z.string().startsWith('/'),
    keywords: z.string().optional(),
    label: z.string(),
    version: z.string().optional(),
  })
  .strict();

const searchVersionSchema = z
  .object({
    channels: z.array(z.string()),
    current: z.boolean(),
    routeVersion: z.string(),
  })
  .strict();

export const generatedSearchIndexSchema = z
  .object({
    entries: z.array(searchEntrySchema),
    versions: z.array(searchVersionSchema),
  })
  .strict();

export type SearchEntry = z.output<typeof searchEntrySchema>;
export type SearchVersion = z.output<typeof searchVersionSchema>;
export type GeneratedSearchIndex = z.output<typeof generatedSearchIndexSchema>;
