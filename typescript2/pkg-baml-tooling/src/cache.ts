import { createHash } from 'node:crypto';
import {
  mkdir,
  readdir,
  readFile,
  rename,
  stat,
  unlink,
  writeFile,
} from 'node:fs/promises';
import { dirname, join } from 'node:path';

const flights = new Map<string, Promise<unknown>>();

/**
 * Entries are immutable and content-addressed, so every source edit writes a
 * new key and old keys are never read again. Without a bound the directory
 * would grow forever across an editing session; the cache is therefore capped
 * (default 512 entries, ~a few MB of JSON) and pruned oldest-write-first.
 * Pruning deletes whole entry files only, so a concurrent `get` racing a
 * pruned key simply misses — bad or stale entries are misses, never errors.
 *
 * The bound is derived from on-disk state, so it holds across processes and
 * not merely within one. The in-memory counter below only amortizes the
 * listing inside a single long-lived process; on its own it would never fire
 * for a short-lived CLI or build that writes a handful of artifacts and
 * exits, and the shared directory would grow past the cap forever, one run at
 * a time. Every process therefore also reconciles against the directory once,
 * on its first write.
 *
 * The bound is amortized, not instantaneous: pruning on every write would
 * cost a listing per write, so the directory may carry up to
 * `PRUNE_EVERY_PUTS` entries beyond the cap between prunes. What matters is
 * that the overshoot is capped by that constant and does not grow with the
 * number of writes, the number of processes, or the age of the directory.
 */
export interface DiskArtifactCacheOptions {
  maxEntries?: number;
}

const DEFAULT_MAX_ENTRIES = 512;
const PRUNE_EVERY_PUTS = 32;

export class DiskArtifactCache {
  readonly #maxEntries: number;
  #putsSincePrune = 0;
  #reconciled = false;

  constructor(
    readonly directory: string,
    options: DiskArtifactCacheOptions = {},
  ) {
    this.#maxEntries = options.maxEntries ?? DEFAULT_MAX_ENTRIES;
  }

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
    // Amortized: the directory listing costs one readdir per shard, so only
    // pay it every PRUNE_EVERY_PUTS writes rather than on every edit. The
    // first write of each process pays it unconditionally — that is what
    // makes the cap survive processes too short-lived to ever reach the
    // threshold. Under the cap the reconcile stops at the readdirs, so a
    // process start never costs a stat per entry.
    const due = ++this.#putsSincePrune >= PRUNE_EVERY_PUTS;
    if (due || !this.#reconciled) {
      this.#putsSincePrune = 0;
      this.#reconciled = true;
      await this.prune().catch(() => undefined);
    }
  }

  async prune(): Promise<void> {
    return this.singleFlight(`prune\0${this.directory}`, async () => {
      const paths: string[] = [];
      let shards: string[] = [];
      try {
        shards = await readdir(this.directory);
      } catch {
        return;
      }
      for (const shard of shards) {
        if (!/^[0-9a-f]{2}$/.test(shard)) continue;
        let names: string[] = [];
        try {
          names = await readdir(join(this.directory, shard));
        } catch {
          continue;
        }
        for (const name of names)
          if (name.endsWith('.json'))
            paths.push(join(this.directory, shard, name));
      }
      // Counting is readdir-only; the stat per entry that oldest-write-first
      // eviction needs is paid only once the directory is actually over cap.
      if (paths.length <= this.#maxEntries) return;
      const entries: { path: string; mtimeMs: number }[] = [];
      for (const path of paths) {
        try {
          entries.push({ mtimeMs: (await stat(path)).mtimeMs, path });
        } catch {
          // Raced with another pruner; skip.
        }
      }
      // Evict to 75% of the cap so the next prune is not triggered by the
      // very next write.
      const target = Math.floor(this.#maxEntries * 0.75);
      if (entries.length <= target) return;
      entries.sort((left, right) => left.mtimeMs - right.mtimeMs);
      for (const entry of entries.slice(0, entries.length - target))
        await unlink(entry.path).catch(() => undefined);
    });
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
