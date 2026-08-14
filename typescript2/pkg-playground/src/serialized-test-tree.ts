// Serialized test tree shape from BAML TestRegistry.serialize().

export type SerializedTest = { type: 'test'; name: string };

export type SerializedLazyTestSet = { type: 'lazyTestSet'; name: string };

export type SerializedTestSet = {
  name: string;
  items: SerializedTestDef[];
  loadingTimeMs: number;
};

export type SerializedTestDef =
  | SerializedTest
  | SerializedLazyTestSet
  | SerializedTestSet;

export function parseSerializedTestTreeJson(json: string): SerializedTestDef[] {
  return normalizeSerializedTestTree(JSON.parse(json));
}

export function normalizeSerializedTestTree(value: unknown): SerializedTestDef[] {
  if (!Array.isArray(value)) {
    throw new Error('Serialized test tree must be an array');
  }
  return value.map((item, index) =>
    normalizeSerializedTestDef(item, `testTree[${index}]`),
  );
}

function normalizeSerializedTestDef(
  value: unknown,
  path: string,
): SerializedTestDef {
  if (!isRecord(value)) {
    throw new Error(`${path} must be an object`);
  }

  const name = readString(value, 'name', path);
  if (value.type === 'test') {
    return { type: 'test', name };
  }
  if (value.type === 'lazyTestSet') {
    return { type: 'lazyTestSet', name };
  }
  if (Array.isArray(value.items)) {
    return {
      name,
      items: value.items.map((item, index) =>
        normalizeSerializedTestDef(item, `${path}.items[${index}]`),
      ),
      loadingTimeMs:
        typeof value.loadingTimeMs === 'number' ? value.loadingTimeMs : 0,
    };
  }

  throw new Error(`${path} must be a test, lazy testset, or expanded testset`);
}

function readString(
  value: Record<string, unknown>,
  key: string,
  path: string,
): string {
  const field = value[key];
  if (typeof field !== 'string' || field.length === 0) {
    throw new Error(`${path}.${key} must be a non-empty string`);
  }
  return field;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}
