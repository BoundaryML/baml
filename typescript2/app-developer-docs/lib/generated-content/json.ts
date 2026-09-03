import { createHash } from 'node:crypto';
import { z } from 'zod';

export type JsonPrimitive = boolean | null | number | string;
export type JsonValue =
  | JsonPrimitive
  | JsonValue[]
  | { [key: string]: JsonValue };

const jsonNumberSchema = z.number();
const nonFiniteNumberSchema = z.union([
  z.nan(),
  z.literal(Number.POSITIVE_INFINITY),
  z.literal(Number.NEGATIVE_INFINITY),
]);
const jsonScalarSchema = z.union([z.null(), z.boolean(), z.string()]);

export const jsonValueSchema: z.ZodType<JsonValue> = z.lazy(() =>
  z.union([
    jsonScalarSchema,
    z.number().finite(),
    z.array(jsonValueSchema),
    z.record(z.string(), jsonValueSchema),
  ]),
);

const jsonObjectSchema = z.record(z.string(), z.custom<JsonValue>());

function normalizeJson(value: JsonValue): JsonValue {
  const scalar = jsonScalarSchema.safeParse(value);
  if (scalar.success) return scalar.data;

  if (nonFiniteNumberSchema.safeParse(value).success) {
    throw new TypeError('Canonical JSON cannot contain non-finite numbers.');
  }

  const number = jsonNumberSchema.safeParse(value);
  if (number.success) {
    return Object.is(number.data, -0) ? 0 : number.data;
  }

  if (Array.isArray(value)) {
    return value.map(normalizeJson);
  }

  const object = jsonObjectSchema.safeParse(value);
  if (!object.success) {
    throw new TypeError('Canonical JSON objects must be plain objects.');
  }

  return Object.fromEntries(
    Object.keys(object.data)
      .sort()
      .map((key) => [key, normalizeJson(object.data[key])]),
  );
}

export function canonicalJson(value: JsonValue): string {
  return JSON.stringify(normalizeJson(value));
}

export function sha256(value: string | Uint8Array): string {
  return createHash('sha256').update(value).digest('hex');
}

export function assertSha256(
  value: string,
  expectedHash: string,
  label: string,
): void {
  const actualHash = sha256(value);
  if (actualHash !== expectedHash) {
    throw new Error(
      `${label} SHA-256 mismatch: expected ${expectedHash}, received ${actualHash}.`,
    );
  }
}
