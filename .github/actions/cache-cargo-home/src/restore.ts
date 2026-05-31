import * as fsp from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import * as core from "./actions.ts";

import {
  CONCURRENCY,
  CRATES_IO_INDEX,
  STATE_MISSED,
  fileExists,
  loadPlan,
  makeStore,
  writeStats,
  type MissedState,
} from "./common.ts";
import { mapPool } from "./pool.ts";
import { tarExtractFromFile } from "./tar.ts";
import {
  registryCacheDir,
  gitDbDir,
  crateObjectKey,
  gitDbListPrefix,
  sha256Hex,
  indexEntryObjectKey,
  indexEntryDiskPath,
  indexConfigObjectKey,
  indexConfigDiskPath,
} from "./cargo-paths.ts";

/**
 * main entrypoint: restore crate `.crate` files and git db tarballs from R2 into
 * $CARGO_HOME, based on what Cargo.lock references. Whatever is *not* in the
 * cache is recorded in the action state so the post step can upload it after the
 * build has fetched it.
 */
async function restore(): Promise<void> {
  const plan = await loadPlan();
  if (!plan) {
    saveMissed({ crates: [], git: [], index: [], indexConfig: false });
    return;
  }
  const store = makeStore();
  if (!store) {
    saveMissed({ crates: [], git: [], index: [], indexConfig: false });
    return;
  }

  core.info(`Cargo.lock: ${plan.cargoLockPath}   CARGO_HOME: ${plan.cargoHome}`);
  core.info(
    `Cargo.lock has ${plan.cratesIo.length} crates.io crates` +
      `${plan.otherRegistry ? ` (+${plan.otherRegistry} other-registry skipped)` : ""} ` +
      `and ${plan.gitRepos.length} git repos.`,
  );

  const cacheDir = registryCacheDir(plan.cargoHome, CRATES_IO_INDEX);
  await fsp.mkdir(cacheDir, { recursive: true });
  await fsp.mkdir(gitDbDir(plan.cargoHome), { recursive: true });

  const startedAt = Date.now();
  const missed: MissedState = { crates: [], git: [], index: [], indexConfig: false };
  const restoredCrateFiles: string[] = [];
  let presentCrates = 0;

  const tCrates = Date.now();
  await mapPool(plan.cratesIo, CONCURRENCY, async (pkg) => {
    const dest = path.join(cacheDir, pkg.crateFile);
    if (await fileExists(dest)) {
      presentCrates++;
      return;
    }
    try {
      const bytes = await store.get(crateObjectKey(CRATES_IO_INDEX, pkg.crateFile));
      if (!bytes) {
        missed.crates.push({ crateFile: pkg.crateFile, checksum: pkg.checksum });
        return;
      }
      if (sha256Hex(bytes) !== pkg.checksum) {
        core.warning(`Checksum mismatch for ${pkg.crateFile} from cache; will refetch.`);
        missed.crates.push({ crateFile: pkg.crateFile, checksum: pkg.checksum });
        return;
      }
      await fsp.writeFile(dest, bytes);
      restoredCrateFiles.push(pkg.crateFile);
    } catch (e) {
      core.warning(`Restore failed for ${pkg.crateFile}: ${(e as Error).message}`);
      missed.crates.push({ crateFile: pkg.crateFile, checksum: pkg.checksum });
    }
  });
  const cratesMs = Date.now() - tCrates;

  // Sparse-index entries: one small file per crate name. Pre-restoring them
  // turns cargo's "Updating crates.io index" from a cold download of every
  // entry into cheap conditional (304) revalidation. A miss is harmless —
  // cargo just fetches that one entry online — so this stays Cargo.lock-driven
  // and tolerant to lockfile changes.
  const indexNames = [...new Set(plan.cratesIo.map((p) => p.name.toLowerCase()))];
  const restoredIndexNames: string[] = [];
  const tIndex = Date.now();
  await mapPool(indexNames, CONCURRENCY, async (name) => {
    const dest = indexEntryDiskPath(plan.cargoHome, CRATES_IO_INDEX, name);
    if (await fileExists(dest)) return;
    try {
      const bytes = await store.get(indexEntryObjectKey(CRATES_IO_INDEX, name));
      if (!bytes) {
        missed.index.push(name); // cargo fetches it online; the post step uploads it.
        return;
      }
      await fsp.mkdir(path.dirname(dest), { recursive: true });
      await fsp.writeFile(dest, bytes);
      restoredIndexNames.push(name);
    } catch (e) {
      core.warning(`Index restore failed for ${name}: ${(e as Error).message}`);
      missed.index.push(name);
    }
  });
  const indexMs = Date.now() - tIndex;

  // The sparse registry's config.json — required for cargo to use the index
  // offline ("config.json not found in registry" otherwise).
  const cfgDisk = indexConfigDiskPath(plan.cargoHome, CRATES_IO_INDEX);
  if (!(await fileExists(cfgDisk))) {
    try {
      const cfg = await store.get(indexConfigObjectKey(CRATES_IO_INDEX));
      if (cfg) {
        await fsp.mkdir(path.dirname(cfgDisk), { recursive: true });
        await fsp.writeFile(cfgDisk, cfg);
      } else {
        missed.indexConfig = true;
      }
    } catch (e) {
      core.warning(`Index config restore failed: ${(e as Error).message}`);
      missed.indexConfig = true;
    }
  }

  const restoredGitObjects: string[] = [];
  const tGit = Date.now();
  await mapPool(plan.gitRepos, CONCURRENCY, async (repo) => {
    try {
      // The on-disk dir is git/db/<ident>-<cargohash>; we can't predict cargo's
      // hash on a cold runner, so list by the ident prefix and fetch the match.
      const tars = (await store.list(gitDbListPrefix(repo.url))).filter((k) => k.endsWith(".tar"));
      if (tars.length === 0) {
        missed.git.push(repo.url);
        return;
      }
      if (tars.length > 1) {
        core.warning(`Multiple git db objects match '${gitDbListPrefix(repo.url)}': ${tars.join(", ")}; using ${tars[0]}.`);
      }
      // Stream the (possibly multi-GB) tar to a temp file, then extract it —
      // never buffer the whole object in memory.
      const tmp = path.join(os.tmpdir(), `cargo-cache-restore-${path.basename(tars[0]!)}-${process.pid}`);
      try {
        const ok = await store.getToFile(tars[0]!, tmp);
        if (!ok) {
          missed.git.push(repo.url);
          return;
        }
        await tarExtractFromFile(plan.cargoHome, tmp);
        restoredGitObjects.push(tars[0]!);
      } finally {
        await fsp.rm(tmp, { force: true });
      }
    } catch (e) {
      core.warning(`Restore failed for git repo ${repo.url}: ${(e as Error).message}`);
      missed.git.push(repo.url);
    }
  });
  const gitMs = Date.now() - tGit;

  const durationMs = Date.now() - startedAt;
  core.info(
    `Restore (${durationMs}ms): crates ${restoredCrateFiles.length} from cache, ${presentCrates} already present, ` +
      `${missed.crates.length} missing; index ${restoredIndexNames.length} entries; ` +
      `git ${restoredGitObjects.length} restored, ${missed.git.length} missing.`,
  );

  const cacheHit = missed.crates.length === 0 && missed.git.length === 0;
  core.setOutput("cache-hit", String(cacheHit));
  core.setOutput("restored-crates", String(restoredCrateFiles.length));
  core.setOutput("present-crates", String(presentCrates));
  core.setOutput("missed-crates", String(missed.crates.length));
  core.setOutput("restored-index", String(restoredIndexNames.length));
  core.setOutput("restored-git", String(restoredGitObjects.length));
  core.setOutput("missed-git", String(missed.git.length));

  const stats = {
    operation: "restore" as const,
    durationMs,
    cargoHome: plan.cargoHome,
    cargoLock: plan.cargoLockPath,
    cacheHit,
    phases: {
      crates: {
        restored: restoredCrateFiles.length,
        present: presentCrates,
        missed: missed.crates.length,
        durationMs: cratesMs,
      },
      index: {
        restored: restoredIndexNames.length,
        missed: missed.index.length,
        configRestored: !missed.indexConfig,
        durationMs: indexMs,
      },
      git: { restored: restoredGitObjects.length, missed: missed.git.length, durationMs: gitMs },
    },
    artifacts: {
      cratesRestored: restoredCrateFiles.sort(),
      cratesMissed: missed.crates.map((c) => c.crateFile).sort(),
      indexRestored: restoredIndexNames.sort(),
      indexMissed: [...missed.index].sort(),
      gitRestored: restoredGitObjects.sort(),
      gitMissed: [...missed.git].sort(),
    },
  };
  const p = await writeStats("restore", stats);
  if (p) core.info(`Wrote restore stats to ${p}`);

  saveMissed(missed);
}

let missedSaved = false;
function saveMissed(missed: MissedState): void {
  missedSaved = true;
  core.saveState(STATE_MISSED, JSON.stringify(missed));
}

// Best-effort cache: a stray async error — e.g. a recycled keep-alive socket
// emitting 'error', which bypasses promise rejection — must NEVER fail the
// build. Guarantee the post step has state to read, then exit cleanly (0).
function bail(label: string, e: unknown): never {
  const msg = e instanceof Error ? (e.stack ?? e.message) : String(e);
  core.warning(`cache-cargo-home restore ${label} (continuing): ${msg}`);
  if (!missedSaved) saveMissed({ crates: [], git: [], index: [], indexConfig: false });
  process.exit(0);
}
process.on("uncaughtException", (e) => bail("uncaught exception", e));
process.on("unhandledRejection", (e) => bail("unhandled rejection", e));

restore().catch((e) => bail("error", e));
