import { exec } from "node:child_process";
import { mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import type { IncomingMessage } from "node:http";
import { promisify } from "node:util";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";

const pkgRoot = dirname(fileURLToPath(import.meta.url));
const run = promisify(exec);

// Regenerate the grammar JSON whenever a typed source under src/ changes, so the
// preview always reflects the latest rules. Vite already watches the emitted
// JSON (it is imported by preview/main.ts) and full-reloads on rewrite.
function regenerateGrammar(): Plugin {
  const build = () =>
    run("pnpm exec tsx scripts/build.ts", { cwd: pkgRoot }).catch((e) => {
      console.error("[grammar] build failed:\n" + (e.stderr || e.message));
      throw e;
    });
  return {
    name: "regenerate-baml-grammar",
    async buildStart() {
      await build();
    },
    configureServer(server) {
      const srcDir = resolve(pkgRoot, "src");
      server.watcher.add(srcDir);
      server.watcher.on("change", async (file) => {
        if (file.startsWith(srcDir)) await build();
      });
    },
  };
}

function readJsonBody(req: IncomingMessage) {
  return new Promise<unknown>((resolveBody, reject) => {
    let body = "";
    req.setEncoding("utf8");
    req.on("data", (chunk) => {
      body += chunk;
    });
    req.on("end", () => {
      try {
        resolveBody(JSON.parse(body || "{}"));
      } catch (error) {
        reject(error);
      }
    });
    req.on("error", reject);
  });
}

function snapshotPreviewApi(): Plugin {
  const fixturesDir = resolve(pkgRoot, "tests/fixtures");
  const snapshotsDir = resolve(pkgRoot, "tests/snapshots");

  const fixtureNames = async () =>
    (await readdir(fixturesDir, { withFileTypes: true }))
      .filter((entry) => entry.isFile() && entry.name.endsWith(".baml"))
      .map((entry) => entry.name)
      .sort();

  const assertFixtureName = async (name: string) => {
    if (basename(name) !== name) {
      throw new Error("Invalid fixture name");
    }

    const names = await fixtureNames();
    if (!names.includes(name)) {
      throw new Error("Unknown fixture");
    }
  };

  const snapshotPath = (name: string) =>
    join(snapshotsDir, `${name}.scope.txt`);

  return {
    name: "snapshot-preview-api",
    configureServer(server) {
      server.middlewares.use(async (req, res, next) => {
        const url = new URL(req.url ?? "/", "http://localhost");

        try {
          if (req.method === "GET" && url.pathname === "/__grammar/fixtures") {
            res.setHeader("content-type", "application/json");
            res.end(JSON.stringify({ fixtures: await fixtureNames() }));
            return;
          }

          if (req.method === "GET" && url.pathname === "/__grammar/fixture") {
            const name = url.searchParams.get("name") ?? "";
            await assertFixtureName(name);

            let snapshot = "";
            try {
              snapshot = await readFile(snapshotPath(name), "utf8");
            } catch {
              // Missing snapshots are represented as empty text in the UI.
            }

            res.setHeader("content-type", "application/json");
            res.end(
              JSON.stringify({
                name,
                source: await readFile(join(fixturesDir, name), "utf8"),
                snapshot,
              }),
            );
            return;
          }

          if (req.method === "POST" && url.pathname === "/__grammar/snapshot") {
            const body = (await readJsonBody(req)) as {
              name?: unknown;
              snapshot?: unknown;
            };
            if (typeof body.name !== "string") {
              throw new Error("Missing fixture name");
            }
            if (typeof body.snapshot !== "string") {
              throw new Error("Missing snapshot text");
            }

            await assertFixtureName(body.name);
            await mkdir(snapshotsDir, { recursive: true });
            await writeFile(snapshotPath(body.name), body.snapshot);

            res.setHeader("content-type", "application/json");
            res.end(JSON.stringify({ ok: true }));
            return;
          }
        } catch (error) {
          res.statusCode = 400;
          res.setHeader("content-type", "application/json");
          res.end(
            JSON.stringify({
              error: error instanceof Error ? error.message : String(error),
            }),
          );
          return;
        }

        next();
      });
    },
  };
}

export default defineConfig({
  root: resolve(pkgRoot, "preview"),
  // Allow importing the generated JSON that lives one level up from `preview/`.
  server: { fs: { allow: [pkgRoot] } },
  plugins: [regenerateGrammar(), snapshotPreviewApi()],
});
