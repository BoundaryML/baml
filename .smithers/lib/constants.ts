// Shared locations, tracked-file enumeration, and timeouts for the
// upstream-ts-tooling workflow (all files here are Smithers scaffolding,
// removed by the workflow's final cleanup task).
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Repo root derived from this file's real location (<root>/.smithers/lib/…),
// so it is stable no matter where the CLI is launched from.
export const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");

// Reproducible on any machine: the TEVM checkout location is a WORKFLOW INPUT
// (tevmRepo), defaulting to a shared temp-dir clone that preflight creates on
// demand (shallow; research reads only). Never a hard-coded personal path.
// Module-level so prompt builders can read it; the workflow body assigns the
// resolved input value before any prompt renders.
export const TEVM_REPO_DEFAULT = join(tmpdir(), "smithers-tevm-monorepo");
let tevmRepo = TEVM_REPO_DEFAULT;
export function setTevmRepo(path: string): void {
  tevmRepo = path;
}
export function tevmRepoPath(): string {
  return tevmRepo;
}

export const TEVM_GIT_URL = "https://github.com/evmts/tevm-monorepo.git";
export const ARTIFACT_DIR_REL = ".smithers/artifacts/upstream-ts-tooling";
export const ARTIFACT_DIR = join(REPO_ROOT, ARTIFACT_DIR_REL);

// Every Smithers boilerplate file this effort commits into the baml repo. The final
// cleanup task deletes EXACTLY this list (plus the untracked artifact dir) and nothing
// else, so unrelated files can never be swept up.
export const BOILERPLATE_TRACKED_FILES = [
  ".smithers/workflows/upstream-ts-tooling.tsx", // the workflow composition script
  ".smithers/workflows/upstream-ts-tooling.validation.test.tsx", // graph/mount-order validation
  ".smithers/package.json",
  ".smithers/.gitignore",
  ".smithers/bunfig.toml", // MDX preload wiring for bun (run + test)
  ".smithers/preload.ts", // registers the smthrs mdxPlugin bun loader
  ".smithers/mdx.d.ts", // *.mdx module declaration for the editor/tsc
  // Shared library modules.
  ".smithers/lib/agents.ts",
  ".smithers/lib/constants.ts",
  ".smithers/lib/helpers.ts",
  ".smithers/lib/plan.ts",
  ".smithers/lib/prompt-fragments.ts",
  // One file per task node (schema + <Task> component).
  ".smithers/tasks/ci-gate.tsx",
  ".smithers/tasks/cleanup-and-verify.tsx",
  ".smithers/tasks/commit-reviews.tsx",
  ".smithers/tasks/commit-series.tsx",
  ".smithers/tasks/create-draft-pr.tsx",
  ".smithers/tasks/design.tsx",
  ".smithers/tasks/finalize-doc.tsx",
  ".smithers/tasks/fix.tsx",
  ".smithers/tasks/implement-guard.tsx",
  ".smithers/tasks/implement.tsx",
  ".smithers/tasks/lane-commit.tsx",
  ".smithers/tasks/merge-lanes.tsx",
  ".smithers/tasks/panel-fix.tsx",
  ".smithers/tasks/preflight.tsx",
  ".smithers/tasks/record-demos.tsx",
  ".smithers/tasks/record-pre-series-head.tsx",
  ".smithers/tasks/research-baml.tsx",
  ".smithers/tasks/research-tevm.tsx",
  ".smithers/tasks/review-site.tsx",
  ".smithers/tasks/review.tsx",
  ".smithers/tasks/signoff-panel.tsx",
  ".smithers/tasks/synthesize-plan.tsx",
  ".smithers/tasks/verify-series.tsx",
  // One MDX file per agent prompt (prose lives here; dynamic pieces are props).
  ".smithers/prompts/commit-reviews.mdx",
  ".smithers/prompts/commit-series.mdx",
  ".smithers/prompts/demos.mdx",
  ".smithers/prompts/design.mdx",
  ".smithers/prompts/finalize-doc.mdx",
  ".smithers/prompts/fix.mdx",
  ".smithers/prompts/implement.mdx",
  ".smithers/prompts/land-lanes.mdx",
  ".smithers/prompts/lane-commit.mdx",
  ".smithers/prompts/panel-fix.mdx",
  ".smithers/prompts/panel-seat.mdx",
  ".smithers/prompts/research-baml.mdx",
  ".smithers/prompts/research-tevm.mdx",
  ".smithers/prompts/review.mdx",
  ".smithers/prompts/site-deploy.mdx",
  ".smithers/prompts/synthesis.mdx",
] as const;

// Untracked, gitignored runtime residue that is safe to delete during the run.
export const BOILERPLATE_UNTRACKED_PATHS = [ARTIFACT_DIR_REL] as const;

// Untracked runtime residue that must OUTLIVE the run (the engine is still writing the
// run database while the final task executes). Never part of any commit or the PR.
export const RUNTIME_RESIDUE_NOTE =
  "Local-only untracked residue intentionally left on disk (the engine still needs it " +
  "while this task runs): .smithers/node_modules/ (resolution shims to the Smithers " +
  "working tree) and .smithers/smithers.db* (this run's database). Both are invisible " +
  "to git and the PR; delete them by hand after the run exits.";

export const RESEARCH_TIMEOUT_MS = 90 * 60_000;
export const DESIGN_TIMEOUT_MS = 60 * 60_000;
export const SYNTH_TIMEOUT_MS = 60 * 60_000;
export const IMPLEMENT_TIMEOUT_MS = 8 * 60 * 60_000;
export const REVIEW_TIMEOUT_MS = 2 * 60 * 60_000;
export const FIX_TIMEOUT_MS = 4 * 60 * 60_000;
export const DOC_TIMEOUT_MS = 45 * 60_000;
export const SERIES_TIMEOUT_MS = 60 * 60_000;
export const HEARTBEAT_MS = 20 * 60_000;
