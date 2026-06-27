import type { RunStoreClient } from './run-store-client';
import type {
  BoundaryId,
  ValueAvailability,
  ValueCodec,
  ValueRef,
} from './worker-protocol';

export interface ValueBodyCacheEntry {
  boundaryId: BoundaryId;
  valueRefId: string;
  codec: ValueCodec;
  availability: ValueAvailability;
  bytes: Uint8Array | null;
  diagnostic: string | null;
}

export interface ValueBodyCache {
  get(boundaryId: BoundaryId, valueRef: ValueRef): ValueBodyCacheEntry | undefined;
  read(boundaryId: BoundaryId, valueRef: ValueRef): Promise<ValueBodyCacheEntry>;
  subscribe(listener: () => void): () => void;
}

export function createValueBodyCache(client: RunStoreClient): ValueBodyCache {
  const entries = new Map<string, ValueBodyCacheEntry>();
  const pending = new Map<string, Promise<ValueBodyCacheEntry>>();
  const listeners = new Set<() => void>();

  function key(boundaryId: BoundaryId, valueRef: ValueRef): string {
    return `${boundaryId}:${valueRef.id}`;
  }

  function notify(): void {
    for (const listener of listeners) listener();
  }

  return {
    get(boundaryId, valueRef) {
      return entries.get(key(boundaryId, valueRef));
    },

    read(boundaryId, valueRef) {
      const cacheKey = key(boundaryId, valueRef);
      const cached = entries.get(cacheKey);
      if (cached) return Promise.resolve(cached);

      const inFlight = pending.get(cacheKey);
      if (inFlight) return inFlight;

      const promise = client
        .readValue(boundaryId, valueRef)
        .then((body) => {
          const entry: ValueBodyCacheEntry = {
            boundaryId,
            valueRefId: body.valueRefId,
            codec: body.codec,
            availability: body.availability,
            bytes: body.bodyBase64 ? base64ToBytes(body.bodyBase64) : null,
            diagnostic: body.diagnostic ?? null,
          };
          entries.set(cacheKey, entry);
          return entry;
        })
        .finally(() => {
          pending.delete(cacheKey);
          notify();
        });

      pending.set(cacheKey, promise);
      return promise;
    },

    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}

function base64ToBytes(encoded: string): Uint8Array {
  const binary = atob(encoded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}
