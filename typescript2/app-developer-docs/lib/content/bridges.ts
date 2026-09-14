import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';

import { load } from 'js-yaml';
import { z } from 'zod';

const compatibilityRowSchema = z
  .object({
    behavior: z.string().min(1),
    concern: z.string().min(1),
  })
  .strict();

const typeMappingSchema = z
  .object({
    baml: z.string().min(1),
    host: z.string().min(1),
    notes: z.string().min(1),
  })
  .strict();

export const bridgeDataSchema = z
  .object({
    compatibility: z.array(compatibilityRowSchema).min(1),
    gotchas: z.array(z.string().min(1)).min(1),
    id: z.string().regex(/^[a-z][a-z0-9-]*$/),
    schemaVersion: z.literal(1),
    transitions: z.array(z.string().min(1)).min(1),
    types: z.array(typeMappingSchema).min(1),
  })
  .strict();

export type BridgeData = z.output<typeof bridgeDataSchema>;

function bridgePath(id: string) {
  if (!/^[a-z][a-z0-9-]*$/.test(id)) {
    throw new Error(`Invalid bridge ID: ${id}`);
  }
  return resolve(process.cwd(), 'content-data', 'bridges', `${id}.yaml`);
}

export async function loadBridgeData(id: string): Promise<BridgeData> {
  const path = bridgePath(id);
  const parsed = bridgeDataSchema.parse(load(await readFile(path, 'utf8')));
  if (parsed.id !== id) {
    throw new Error(`Bridge ID mismatch: requested ${id}, found ${parsed.id}`);
  }
  return parsed;
}
