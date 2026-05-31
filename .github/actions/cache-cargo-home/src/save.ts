import * as fsp from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { spawn } from "node:child_process";
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
import { tarCreateToFile } from "./tar.ts";
import {
  registryCacheDir,
  gitDbDir,
  crateObjectKey,
  gitDbTarKey,
  gitIdent,
  sha256Hex,
  indexEntryObjectKey,
  indexEntryDiskPath,
  indexConfigObjectKey,
  indexConfigDiskPath,
} from "./cargo-paths.ts";

function readMissed(): MissedState {
  const raw = core.getState(STATE_MISSED);
  if (!raw) return { crates: [], git: [], index: [], indexConfig: false };
  try {
    const v = JSON.parse(raw) as MissedState;
    return {
      crates: v.crates ?? [],
      git: v.git ?? [],
      index: v.index ?? [],
      indexConfig: v.indexConfig ?? false,
    };
  } catch {
    return { crates: [], git: [], index: [], indexConfig: false };
  }
}

/**
 * post entrypoint: upload the objects that were missing at restore time and that
 * the build has since fetched into $CARGO_HOME. Writes are new-only — we HEAD
 * first so the many jobs that all save the same content-addressed crate don't
 * re-upload it (and don't stampede a key another job already wrote).
 */
async function save(): Promise<void> {
  const missed = readMissed();
  if (
    missed.crates.length === 0 &&
    missed.git.length === 0 &&
    missed.index.length === 0 &&
    !missed.indexConfig
  ) {
    core.info("Nothing new to save to the cargo cache.");
    return;
  }

  const plan = await loadPlan();
  if (!plan) return;
  const store = makeStore();
  if (!store) return;

  const startedAt = Date.now();
  const cacheDir = registryCacheDir(plan.cargoHome, CRATES_IO_INDEX);
  const uploadedCrateFiles: string[] = [];

  const tCrates = Date.now();
  await mapPool(missed.crates, CONCURRENCY, async (c) => {
    const src = path.join(cacheDir, c.crateFile);
    try {
      if (!(await fileExists(src))) return; // build didn't end up fetching it.
      const key = crateObjectKey(CRATES_IO_INDEX, c.crateFile);
      if (await store.has(key)) return; // another job already uploaded it.
      const bytes = await fsp.readFile(src);
      if (sha256Hex(bytes) !== c.checksum) {
        core.warning(`Refusing to upload ${c.crateFile}: local checksum mismatch.`);
        return;
      }
      await store.put(key, bytes, "application/x-tar");
      uploadedCrateFiles.push(c.crateFile);
    } catch (e) {
      core.warning(`Upload failed for ${c.crateFile}: ${(e as Error).message}`);
    }
  });
  const cratesMs = Date.now() - tCrates;

  // Sparse-index entries that weren't cached at restore: cargo fetched them
  // online, so upload them now (putIfChanged HEADs first, so concurrent jobs
  // don't clobber an identical object). One small object per crate name.
  const uploadedIndexNames: string[] = [];
  const tIndex = Date.now();
  await mapPool(missed.index, CONCURRENCY, async (name) => {
    const src = indexEntryDiskPath(plan.cargoHome, CRATES_IO_INDEX, name);
    try {
      if (!(await fileExists(src))) return; // cargo didn't end up needing it.
      const bytes = await fsp.readFile(src);
      if (await store.putIfChanged(indexEntryObjectKey(CRATES_IO_INDEX, name), bytes, "text/plain")) {
        uploadedIndexNames.push(name);
      }
    } catch (e) {
      core.warning(`Index upload failed for ${name}: ${(e as Error).message}`);
    }
  });

  // The sparse-index config.json, if it wasn't already cached.
  let uploadedConfig = false;
  if (missed.indexConfig) {
    try {
      const cfgDisk = indexConfigDiskPath(plan.cargoHome, CRATES_IO_INDEX);
      if (await fileExists(cfgDisk)) {
        uploadedConfig = await store.putIfChanged(
          indexConfigObjectKey(CRATES_IO_INDEX),
          await fsp.readFile(cfgDisk),
          "application/json",
        );
      }
    } catch (e) {
      core.warning(`Index config upload failed: ${(e as Error).message}`);
    }
  }
  const indexMs = Date.now() - tIndex;

  // The pinned commit for each git url, used to reject incomplete clones below.
  const revByUrl = new Map<string, string | null>();
  for (const g of plan.gitRepos) revByUrl.set(g.url, g.rev);

  // Map git db directories on disk back to the URLs that need uploading. We
  // don't know cargo's URL-hash suffix, so match the on-disk dir by its ident.
  const dbRoot = gitDbDir(plan.cargoHome);
  let dbDirs: string[] = [];
  try {
    dbDirs = (await fsp.readdir(dbRoot, { withFileTypes: true }))
      .filter((d) => d.isDirectory())
      .map((d) => d.name);
  } catch {
    dbDirs = [];
  }

  // Git db dirs can be multi-GB (e.g. the aws-sdk-rust fork), so each is tarred
  // to a temp file and multipart-uploaded from disk — never buffered/hashed in
  // one shot. Keep git concurrency low so we don't pile several huge temp tars
  // on disk at once (each putLargeFile already parallelizes its own parts).
  const uploadedGitObjects: string[] = [];
  const tGit = Date.now();
  await mapPool(missed.git, 2, async (url) => {
    const ident = gitIdent(url);
    const matches = dbDirs.filter((d) => d === ident || d.startsWith(`${ident}-`));
    if (matches.length === 0) {
      core.warning(`No git db directory found for ${url} (ident '${ident}').`);
      return;
    }
    const rev = revByUrl.get(url) ?? null;
    try {
      for (const dir of matches) {
        // Object key mirrors the on-disk dir name: git-db/<ident>-<cargohash>.tar
        const key = gitDbTarKey(dir);
        if (await store.has(key)) continue; // already uploaded by another job.
        // Never cache an incomplete clone: if cargo's fetch was interrupted
        // (e.g. the debug job's `cargo fetch` timeout), the db is missing the
        // pinned commit and caching it would poison every future restore.
        if (rev && !(await gitDbHasCommit(path.join(dbRoot, dir), rev))) {
          core.warning(`Skipping ${dir}: pinned rev ${rev} not present (incomplete clone).`);
          continue;
        }
        const tmp = path.join(os.tmpdir(), `cargo-cache-${dir}-${process.pid}.tar`);
        try {
          // Forward-slash archive member path so a tar written on one OS extracts
          // correctly on another (the cache is shared across linux/macos/windows).
          const bytes = await tarCreateToFile(plan.cargoHome, ["git", "db", dir].join("/"), tmp);
          core.info(`Uploading git db ${dir} (${(bytes / 1e6).toFixed(0)} MB) to ${key}`);
          await store.putLargeFile(key, tmp, "application/x-tar");
          uploadedGitObjects.push(key);
        } finally {
          await fsp.rm(tmp, { force: true });
        }
      }
    } catch (e) {
      core.warning(`Upload failed for git repo ${url}: ${(e as Error).message}`);
    }
  });

  const gitMs = Date.now() - tGit;
  const durationMs = Date.now() - startedAt;
  core.info(
    `Saved (${durationMs}ms) ${uploadedCrateFiles.length} new crate(s), ` +
      `${uploadedIndexNames.length} index entr(ies)${uploadedConfig ? " + config.json" : ""} and ` +
      `${uploadedGitObjects.length} git db(s) to R2.`,
  );
  core.setOutput("uploaded-crates", String(uploadedCrateFiles.length));
  core.setOutput("uploaded-index", String(uploadedIndexNames.length));
  core.setOutput("uploaded-git", String(uploadedGitObjects.length));

  const stats = {
    operation: "save" as const,
    durationMs,
    cargoHome: plan.cargoHome,
    cargoLock: plan.cargoLockPath,
    phases: {
      crates: { uploaded: uploadedCrateFiles.length, candidates: missed.crates.length, durationMs: cratesMs },
      index: {
        uploaded: uploadedIndexNames.length,
        candidates: missed.index.length,
        configUploaded: uploadedConfig,
        durationMs: indexMs,
      },
      git: { uploaded: uploadedGitObjects.length, candidates: missed.git.length, durationMs: gitMs },
    },
    artifacts: {
      cratesUploaded: uploadedCrateFiles.sort(),
      indexUploaded: uploadedIndexNames.sort(),
      gitUploaded: uploadedGitObjects.sort(),
      configUploaded: uploadedConfig,
    },
  };
  const p = await writeStats("save", stats);
  if (p) core.info(`Wrote save stats to ${p}`);
}

/**
 * True if the bare git db at `dbDir` contains the commit `rev`. A clone that
 * cargo finished has the pinned commit; one interrupted mid-fetch does not.
 * On any spawn error (git missing, etc.) we return true so we never block a
 * legitimate upload on an environment quirk.
 */
function gitDbHasCommit(dbDir: string, rev: string): Promise<boolean> {
  return new Promise((resolve) => {
    let child;
    try {
      child = spawn("git", ["--git-dir", dbDir, "cat-file", "-e", `${rev}^{commit}`], {
        stdio: "ignore",
      });
    } catch {
      resolve(true);
      return;
    }
    child.on("error", () => resolve(true));
    child.on("close", (code) => resolve(code === 0));
  });
}

// Best-effort cache: a stray async error (e.g. a recycled keep-alive socket
// emitting 'error', which bypasses promise rejection) must never fail the build.
function bail(label: string, e: unknown): never {
  const msg = e instanceof Error ? (e.stack ?? e.message) : String(e);
  core.warning(`cache-cargo-home save ${label} (continuing): ${msg}`);
  process.exit(0);
}
process.on("uncaughtException", (e) => bail("uncaught exception", e));
process.on("unhandledRejection", (e) => bail("unhandled rejection", e));

save().catch((e) => bail("error", e));
