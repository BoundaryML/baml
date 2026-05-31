import { createHash } from "node:crypto";
import { homedir } from "node:os";
import * as path from "node:path";

/** Resolve CARGO_HOME, honouring the env var, the action input, and the default. */
export function resolveCargoHome(input?: string): string {
  const raw = (input && input.trim()) || process.env.CARGO_HOME || path.join(homedir(), ".cargo");
  if (raw.startsWith("~")) {
    return path.join(homedir(), raw.slice(1));
  }
  return path.resolve(raw);
}

/** The registry `.crate` cache directory for a given index, e.g.
 *  ~/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f */
export function registryCacheDir(cargoHome: string, indexName: string): string {
  return path.join(cargoHome, "registry", "cache", indexName);
}

/** ~/.cargo/git/db */
export function gitDbDir(cargoHome: string): string {
  return path.join(cargoHome, "git", "db");
}

export function sha256Hex(data: Buffer | Uint8Array | string): string {
  return createHash("sha256").update(data).digest("hex");
}

/**
 * cargo names a git db directory `<ident>-<hash>` where ident is the last path
 * segment of the URL (minus a trailing `.git`). We can't reproduce cargo's hash
 * portably, but we *can* recognise the ident to match a URL to an on-disk dir.
 */
export function gitIdent(url: string): string {
  let u = url;
  // Drop any query string / fragment.
  u = u.replace(/[?#].*$/, "");
  u = u.replace(/\/+$/, "");
  let last = u.split("/").pop() ?? u;
  if (last.endsWith(".git")) last = last.slice(0, -4);
  return last.toLowerCase();
}

/**
 * R2 object key for a registry crate, mirroring its on-disk cache path so the
 * key is human-readable and lines up 1:1 with $CARGO_HOME:
 *   registry/cache/<index>/<name>-<ver>.crate  ->  crates/<index>/<name>-<ver>.crate
 * Integrity is still checked against Cargo.lock's recorded sha256 after download.
 */
export function crateObjectKey(indexName: string, crateFile: string): string {
  return `crates/${indexName}/${crateFile}`;
}

/**
 * R2 object key for a git db tarball, mirroring its on-disk directory name:
 *   git/db/<ident>-<cargohash>  ->  git-db/<ident>-<cargohash>.tar
 * The dir name (incl. cargo's URL-hash suffix) is known at save time from disk.
 */
export function gitDbTarKey(dirName: string): string {
  return `git-db/${dirName}.tar`;
}

/**
 * Prefix used to discover a repo's git db tarball at restore time. We can't
 * reproduce cargo's `<ident>-<hash>` dir name from the URL on a cold runner, so
 * we list by the ident prefix (e.g. `git-db/aws-sdk-rust-`) and fetch the match.
 */
export function gitDbListPrefix(url: string): string {
  return `git-db/${gitIdent(url)}-`;
}

/**
 * cargo's sharded path for a crate's sparse-index `.cache` entry, as path
 * segments (lowercased): 1-char `1/<n>`, 2-char `2/<n>`, 3-char `3/<c>/<n>`,
 * 4+-char `<c1c2>/<c3c4>/<n>` — e.g. serde -> se/rd/serde, h2 -> 2/h2.
 */
function indexEntryRelParts(name: string): string[] {
  const n = name.toLowerCase();
  if (n.length === 1) return ["1", n];
  if (n.length === 2) return ["2", n];
  if (n.length === 3) return ["3", n[0]!, n];
  return [n.slice(0, 2), n.slice(2, 4), n];
}

/**
 * R2 object key for a single crate's sparse-index entry, mirroring its on-disk
 * `.cache` path so the cache is piecemeal (one object per crate name) and shared
 * across any Cargo.lock: registry/index/<index>/.cache/se/rd/serde -> registry-index/<index>/se/rd/serde
 */
export function indexEntryObjectKey(indexName: string, name: string): string {
  return `registry-index/${indexName}/${indexEntryRelParts(name).join("/")}`;
}

/** On-disk path of a crate's sparse-index `.cache` entry under CARGO_HOME. */
export function indexEntryDiskPath(cargoHome: string, indexName: string, name: string): string {
  return path.join(cargoHome, "registry", "index", indexName, ".cache", ...indexEntryRelParts(name));
}

/**
 * The sparse registry's `config.json` (tiny, static). cargo refuses to use the
 * index offline without it ("config.json not found in registry"), so it's
 * cached alongside the per-crate entries.
 */
export function indexConfigObjectKey(indexName: string): string {
  return `registry-index/${indexName}/config.json`;
}

export function indexConfigDiskPath(cargoHome: string, indexName: string): string {
  return path.join(cargoHome, "registry", "index", indexName, "config.json");
}
