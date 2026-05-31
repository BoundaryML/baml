import * as fs from "node:fs";
import * as fsp from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import * as core from "./actions.ts";

import { parseCargoLock, isCratesIo, type GitPackage, type RegistryPackage } from "./cargo-lock.ts";
import { resolveR2Config, R2Store } from "./r2.ts";
import { resolveCargoHome } from "./cargo-paths.ts";

// This action takes NO inputs. Everything here is a fixed convention of this
// repo or is derived from Cargo.lock / the environment at runtime.

/** The crate we cache for, relative to the checkout root. */
export const WORKSPACE_SUBDIR = "baml_language";
/** crates.io sparse-index directory name (stable for years). */
export const CRATES_IO_INDEX = "index.crates.io-1949cf8c6b5b557f";
/** Namespace under the (sccache) key prefix for everything this action stores. */
export const KEY_NAMESPACE = "cargo-home";
/** Max concurrent R2 requests. */
export const CONCURRENCY = 48;

/** core.saveState keys shared between the main (restore) and post (save) runs. */
export const STATE_MISSED = "missed";

export interface MissedState {
  crates: Array<{ crateFile: string; checksum: string }>;
  git: string[]; // canonical urls
  index: string[]; // crate names whose sparse-index entry wasn't cached
  indexConfig: boolean; // the sparse-index config.json wasn't cached
}

export interface Plan {
  cargoLockPath: string;
  manifestDir: string;
  cargoHome: string;
  cratesIo: RegistryPackage[];
  gitRepos: GitPackage[];
  otherRegistry: number;
}

function repoRoot(): string {
  return process.env.GITHUB_WORKSPACE || process.cwd();
}

export async function fileExists(p: string): Promise<boolean> {
  try {
    await fsp.access(p, fs.constants.F_OK);
    return true;
  } catch {
    return false;
  }
}

/** Locate baml_language/Cargo.lock, falling back to a shallow search. */
async function findCargoLock(): Promise<string | null> {
  const candidates = [
    path.join(repoRoot(), WORKSPACE_SUBDIR, "Cargo.lock"),
    path.join(process.cwd(), WORKSPACE_SUBDIR, "Cargo.lock"),
    path.join(process.cwd(), "Cargo.lock"),
  ];
  for (const c of candidates) {
    if (await fileExists(c)) return c;
  }
  return null;
}

/**
 * Read Cargo.lock and decide what to cache. Returns null if the lockfile is
 * missing (a warning is logged). The lockfile is the *only* thing that decides
 * which directories under ~/.cargo/git and ~/.cargo/registry we touch:
 *   - crates.io packages -> registry/cache/<index>/<name>-<ver>.crate
 *   - git packages       -> git/db/<repo dir>
 */
export async function loadPlan(): Promise<Plan | null> {
  const cargoLockPath = await findCargoLock();
  if (!cargoLockPath) {
    core.warning(`Could not find ${WORKSPACE_SUBDIR}/Cargo.lock; nothing to cache.`);
    return null;
  }
  const lock = parseCargoLock(await fsp.readFile(cargoLockPath, "utf8"));
  const cratesIo = lock.registry.filter((p) => isCratesIo(p.source));
  const otherRegistry = lock.registry.length - cratesIo.length;

  // One db tar per repo, shared across the revisions that repo is pinned to.
  const byUrl = new Map<string, GitPackage>();
  for (const g of lock.git) if (!byUrl.has(g.url)) byUrl.set(g.url, g);

  return {
    cargoLockPath,
    manifestDir: path.dirname(cargoLockPath),
    cargoHome: resolveCargoHome(),
    cratesIo,
    gitRepos: [...byUrl.values()],
    otherRegistry,
  };
}

/**
 * Where the restore/save stats JSON is written. Override with
 * CARGO_CACHE_STATS_DIR; otherwise the runner temp dir (or the OS temp dir).
 */
export function statsPath(operation: "restore" | "save"): string {
  const dir = process.env.CARGO_CACHE_STATS_DIR || process.env.RUNNER_TEMP || os.tmpdir();
  return path.join(dir, `cache-cargo-home-${operation}.json`);
}

/** Write a stats object as pretty JSON; returns the path, or null on failure. */
export async function writeStats(operation: "restore" | "save", stats: unknown): Promise<string | null> {
  try {
    const p = statsPath(operation);
    await fsp.mkdir(path.dirname(p), { recursive: true });
    await fsp.writeFile(p, JSON.stringify(stats, null, 2) + "\n");
    core.setOutput(`${operation}-stats-path`, p);
    return p;
  } catch (e) {
    core.warning(`Failed to write ${operation} stats: ${(e as Error).message}`);
    return null;
  }
}

/** Build an R2 store from the environment, or null if R2 isn't configured. */
export function makeStore(): R2Store | null {
  const cfg = resolveR2Config(KEY_NAMESPACE);
  if (!cfg) {
    core.warning(
      "R2 not configured (need SCCACHE_ENDPOINT, SCCACHE_BUCKET and " +
        "BAML_SCCACHE_R2_ACCESS_KEY_ID / BAML_SCCACHE_R2_SECRET_ACCESS_KEY in the " +
        "environment). Skipping cargo cache, same as sccache's secretless fallback.",
    );
    return null;
  }
  core.info(`R2 endpoint ${cfg.endpoint}  bucket ${cfg.bucket}  prefix ${cfg.keyPrefix || "(none)"}`);
  return new R2Store(cfg);
}
