import { createHash } from 'node:crypto';
import { mkdir, readFile, rename, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';

const flights = new Map<string, Promise<unknown>>();

export class DiskArtifactCache {
  constructor(readonly directory: string) {}

  key(parts: readonly (string | Uint8Array)[]): string {
    const hash = createHash('sha256');
    hash.update('baml.tooling.v1\0');
    for (const part of parts) hash.update(part).update('\0');
    return hash.digest('hex');
  }

  async get<T>(key: string): Promise<T | undefined> {
    try {
      const envelope = JSON.parse(await readFile(this.path(key), 'utf8')) as {
        version: 1;
        key: string;
        value: T;
      };
      return envelope.version === 1 && envelope.key === key
        ? envelope.value
        : undefined;
    } catch {
      return undefined;
    }
  }

  async put<T>(key: string, value: T): Promise<void> {
    const path = this.path(key);
    await mkdir(dirname(path), { recursive: true });
    const temporary = `${path}.${process.pid}.${Math.random().toString(16).slice(2)}.tmp`;
    await writeFile(temporary, JSON.stringify({ key, value, version: 1 }));
    await rename(temporary, path);
  }

  singleFlight<T>(key: string, produce: () => Promise<T>): Promise<T> {
    const existing = flights.get(key) as Promise<T> | undefined;
    if (existing) return existing;
    const pending = produce().finally(() => flights.delete(key));
    flights.set(key, pending);
    return pending;
  }

  path(key: string): string {
    return join(this.directory, key.slice(0, 2), `${key}.json`);
  }
}
