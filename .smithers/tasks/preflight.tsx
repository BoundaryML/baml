// Task 0. Preflight: discover adapters/CLIs (fail clearly if absent), auth, fork,
// feature branch, and the early scaffolding commit (requirement: the workflow
// script is committed well before cleanup).
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { existsSync, mkdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { z } from "zod/v4";
import { ARTIFACT_DIR, BOILERPLATE_TRACKED_FILES, TEVM_GIT_URL, tevmRepoPath } from "../lib/constants.ts";
import { sh, trySh } from "../lib/helpers.ts";

export const preflightSchema = z.object({
  login: z.string().min(1),
  forkUrl: z.string().min(10),
  branch: z.string().min(1),
  scaffoldCommit: z.string().min(7),
  toolVersions: z.array(z.string()).min(4),
  notes: z.string().default(""),
});

type PreflightProps = {
  outputs: any;
  input: { upstreamRepo: string; baseBranch: string; featureBranch: string };
};

export function Preflight({ outputs, input }: PreflightProps) {
  return (
    <Task
      id="preflight"
      output={outputs.bttPreflight}
      sideEffect={{ idempotent: true }}
      timeoutMs={10 * 60_000}
      retries={1}
    >
      {async () => {
        // 1) Adapter discovery: fail with one clear message naming everything missing.
        const required: Array<[tool: string, args: string[], hint: string]> = [
          ["codex", ["--version"], "install/authenticate the OpenAI Codex CLI (Codex Sol seat)"],
          ["claude", ["--version"], "install/authenticate the Claude Code CLI (Claude Fable seat)"],
          ["opencode", ["--version"], "install/authenticate the OpenCode CLI (Kimi K3 seat)"],
          ["gh", ["--version"], "install the GitHub CLI and run `gh auth login`"],
          ["git", ["--version"], "install git"],
        ];
        const missing: string[] = [];
        const toolVersions: string[] = [];
        for (const [tool, args, hint] of required) {
          const out = trySh(tool, args);
          if (out === null) missing.push(`${tool}: NOT FOUND on PATH. Fix: ${hint}.`);
          else toolVersions.push(`${tool} ${out.trim().split("\n")[0]}`);
        }
        if (missing.length > 0) {
          throw new Error(`Required agent/tool CLIs are absent:\n${missing.join("\n")}`);
        }
        if (trySh("gh", ["auth", "status"]) === null) {
          throw new Error("gh is installed but not authenticated. Run `gh auth login` and re-run.");
        }
        // Reproducibility: materialize the TEVM checkout at the input path if it
        // is not already there (shallow — research only reads it).
        if (!existsSync(join(tevmRepoPath(), ".git"))) {
          mkdirSync(dirname(tevmRepoPath()), { recursive: true });
          sh("git", ["clone", "--depth", "1", TEVM_GIT_URL, tevmRepoPath()]);
        }
        // Retro change 7: version minimums and toolchain pins. A stale OpenCode
        // cost a design round; `typescript@latest` on npm is now the native Go
        // build (no tsserver.js), so the tsserver plugin work must pin TS 5.x.
        let notes = "";
        const opencodeVer = trySh("opencode", ["--version"])?.trim() ?? "0";
        const opencodeMajor = Number(opencodeVer.split(".")[0]) || 0;
        if (opencodeMajor < 1) {
          throw new Error(
            `OpenCode ${opencodeVer} is too old (needs >= 1.x; a stale OpenCode broke the Kimi seat's CLI contract on the first run). Update: npm install -g opencode-ai@latest`,
          );
        }
        for (const [tool, args] of [
          ["cargo", ["--version"]],
          ["pnpm", ["--version"]],
          ["node", ["--version"]],
          ["vhs", ["--version"]],
          ["wrangler", ["--version"]],
        ] as Array<[string, string[]]>) {
          const out = trySh(tool, args);
          if (out === null) notes += `${tool} missing (needed later: vhs for demos, wrangler for the review site). `;
          else toolVersions.push(`${tool} ${out.trim().split("\n")[0]}`);
        }
        notes +=
          "Toolchain pin: npm `typescript@latest` is the native Go build with no tsserver.js; tsserver-plugin work and its tests must pin typescript@5.x. ";
        const models = trySh("opencode", ["models"]);
        if (models !== null && !models.includes("kimi-for-coding/k3")) {
          throw new Error(
            "OpenCode is installed but the Kimi K3 seat (kimi-for-coding/k3) is not configured. " +
              "Configure the kimi-for-coding provider in OpenCode (opencode auth login), then re-run.",
          );
        }
        if (models === null) notes += "opencode model listing unavailable; kimi-for-coding/k3 presence not pre-verified. ";

        // 2) Fork of the upstream repo (user-authorized), plus a `fork` remote.
        const login = sh("gh", ["api", "user", "-q", ".login"]).trim();
        if (!login) throw new Error("Could not resolve GitHub login via `gh api user`.");
        const [, upstreamName = "baml"] = input.upstreamRepo.split("/");
        const forkSlug = `${login}/${upstreamName}`;
        if (trySh("gh", ["repo", "view", forkSlug, "--json", "name"]) === null) {
          sh("gh", ["repo", "fork", input.upstreamRepo, "--clone=false"]);
          notes += `Created fork ${forkSlug}. `;
        }
        const forkUrl = `https://github.com/${forkSlug}.git`;
        if (trySh("git", ["remote", "get-url", "fork"]) === null) {
          sh("git", ["remote", "add", "fork", forkUrl]);
        } else {
          sh("git", ["remote", "set-url", "fork", forkUrl]);
        }

        // 3) Feature branch off the upstream base.
        sh("git", ["fetch", "origin", input.baseBranch]);
        const current = sh("git", ["rev-parse", "--abbrev-ref", "HEAD"]).trim();
        if (current !== input.featureBranch) {
          if (trySh("git", ["rev-parse", "--verify", input.featureBranch]) !== null) {
            sh("git", ["checkout", input.featureBranch]);
          } else {
            sh("git", ["checkout", "-b", input.featureBranch, `origin/${input.baseBranch}`]);
          }
        }

        // 4) Early scaffolding commit: exactly the enumerated boilerplate files.
        // Fresh-run hygiene first: a previous run's artifacts must never satisfy
        // this run's research/design reads. Resume never re-executes a completed
        // preflight, so a resumed run's own artifacts are preserved.
        rmSync(ARTIFACT_DIR, { recursive: true, force: true });
        mkdirSync(ARTIFACT_DIR, { recursive: true });
        const alreadyTracked = trySh("git", ["ls-files", "--error-unmatch", BOILERPLATE_TRACKED_FILES[0]]) !== null;
        if (!alreadyTracked) {
          sh("git", ["add", "--", ...BOILERPLATE_TRACKED_FILES]);
          sh("git", [
            "commit",
            "-m",
            "chore: add temporary smithers workflow scaffolding (removed by this branch's final commit)",
            "--",
            ...BOILERPLATE_TRACKED_FILES,
          ]);
        }
        const scaffoldCommit = sh("git", ["rev-parse", "HEAD"]).trim();
        return {
          login,
          forkUrl,
          branch: input.featureBranch,
          scaffoldCommit,
          toolVersions,
          notes: notes || "all adapters present",
        };
      }}
    </Task>
  );
}
