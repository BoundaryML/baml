import { readFile } from 'node:fs/promises';

import { load } from 'js-yaml';
import { z } from 'zod';

const allowlistSchema = z
  .object({
    packages: z.array(z.string().regex(/^[A-Za-z_][A-Za-z0-9_]*$/)).min(1),
  })
  .strict()
  .superRefine((value, context) => {
    if (new Set(value.packages).size !== value.packages.length) {
      context.addIssue({
        code: 'custom',
        message: 'Package allowlist contains duplicates.',
      });
    }
  });

const allowlistUrl = new URL(
  '../../content-data/reference/stdlib-packages.yaml',
  import.meta.url,
);

export async function readStandardPackageAllowlist(): Promise<string[]> {
  const source = await readFile(allowlistUrl, 'utf8');
  return allowlistSchema.parse(load(source)).packages;
}
