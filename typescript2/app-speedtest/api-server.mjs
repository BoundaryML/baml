/**
 * Tiny API server — reads ~/.speedtest/ and serves run data.
 * Vite proxies /api/* here during dev.
 *
 * GET /api/runs   — all runs + baselines with ref paths
 */

import { readdir, readFile, lstat, realpath } from "node:fs/promises";
import { createServer } from "node:http";
import { join, resolve, relative } from "node:path";
import { existsSync } from "node:fs";
import { homedir } from "node:os";

function findResultsDir() {
  if (process.env.SPEEDTEST_RESULTS_DIR) {
    return resolve(process.env.SPEEDTEST_RESULTS_DIR);
  }
  return join(homedir(), ".speedtest");
}

const RESULTS_DIR = findResultsDir();

async function loadRuns() {
  const runs = new Map(); // run_id -> data (dedup since baselines are symlinks)
  const refs = [];        // {ref, branch, tag, run_id, type}

  // Load all actual runs
  const runsDir = join(RESULTS_DIR, "runs");
  if (existsSync(runsDir)) {
    const items = await readdir(runsDir, { withFileTypes: true });
    for (const item of items) {
      if (!item.isDirectory()) continue;
      const metaPath = join(runsDir, item.name, "meta.json");
      if (!existsSync(metaPath)) continue;
      try {
        const data = JSON.parse(await readFile(metaPath, "utf-8"));
        runs.set(item.name, { ...data, run_id: item.name });
      } catch { /* skip */ }
    }
  }

  // Scan baselines — each is baselines/<branch>/<tag> -> symlink to run
  const blDir = join(RESULTS_DIR, "baselines");
  if (existsSync(blDir)) {
    const branches = await readdir(blDir, { withFileTypes: true });
    for (const branchEntry of branches) {
      if (!branchEntry.isDirectory()) continue;
      const branch = branchEntry.name;
      const branchPath = join(blDir, branch);
      const tags = await readdir(branchPath, { withFileTypes: true });
      for (const tagEntry of tags) {
        const tagPath = join(branchPath, tagEntry.name);
        // Resolve symlink to find the actual run
        let runId = null;
        try {
          const stat = await lstat(tagPath);
          if (stat.isSymbolicLink()) {
            const real = await realpath(tagPath);
            // Extract run ID from the resolved path (last segment)
            runId = real.split("/").pop();
          } else if (tagEntry.isDirectory()) {
            // Not a symlink — standalone baseline dir
            const metaPath = join(tagPath, "meta.json");
            if (existsSync(metaPath)) {
              const data = JSON.parse(await readFile(metaPath, "utf-8"));
              runId = data.id || tagEntry.name;
              if (!runs.has(runId)) {
                runs.set(runId, { ...data, run_id: runId });
              }
            }
          }
        } catch { continue; }

        if (runId) {
          refs.push({
            ref: `${branch}/${tagEntry.name}`,
            branch,
            tag: tagEntry.name,
            run_id: runId,
          });
        }
      }
    }
  }

  // Build response: runs array + refs array
  const runsArray = [...runs.values()].sort((a, b) =>
    (b.timestamp ?? "").localeCompare(a.timestamp ?? "")
  );

  return { runs: runsArray, refs };
}

const API_PORT = Number(process.env.SPEEDTEST_API_PORT) || 4444;

const server = createServer(async (req, res) => {
  if (req.url === "/api/runs") {
    const data = await loadRuns();
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify(data));
  } else {
    res.writeHead(404);
    res.end("Not found");
  }
});

server.listen(API_PORT, () => {
  console.log(`speedtest API: http://localhost:${API_PORT}`);
  console.log(`Results dir:   ${RESULTS_DIR}`);
});
