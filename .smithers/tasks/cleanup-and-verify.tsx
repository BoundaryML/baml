// Task 8. FINAL task: delete every piece of Smithers boilerplate this effort
// introduced (explicit enumerated list, including the workflow files), commit that
// deletion as the branch's final commit, push it, and verify the draft PR includes
// the cleanup. Mounted by the workflow only after the full panel LGTMs.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { existsSync, rmSync } from "node:fs";
import { resolve } from "node:path";
import { z } from "zod/v4";
import {
  BOILERPLATE_TRACKED_FILES,
  BOILERPLATE_UNTRACKED_PATHS,
  REPO_ROOT,
  RUNTIME_RESIDUE_NOTE,
} from "../lib/constants.ts";
import { asStr, pick, sh, sleep, trySh } from "../lib/helpers.ts";

export const cleanupSchema = z.object({
  deletedTracked: z.array(z.string()).min(3),
  deletedUntracked: z.array(z.string()).default([]),
  cleanupSha: z.string().min(7),
  prNumber: z.number().int().min(1),
  prUrl: z.string().min(10),
  prHeadSha: z.string().min(7),
  prIncludesCleanup: z.boolean(),
  prStillDraft: z.boolean(),
  notes: z.string().min(10),
});

type CleanupProps = {
  outputs: any;
  input: { upstreamRepo: string; featureBranch: string };
};

export function CleanupAndVerify({ outputs, input }: CleanupProps) {
  return (
    <Task
      id="cleanup-and-verify"
      output={outputs.bttCleanup}
      sideEffect={{ idempotent: true }}
      needs={{ pre: "preflight", pr: "create-draft-pr" }}
      deps={{ pre: outputs.bttPreflight, pr: outputs.bttPr }}
      timeoutMs={20 * 60_000}
      retries={2}
      retryPolicy={{ backoff: "exponential", initialDelayMs: 10_000 }}
    >
      {async (deps: any) => {
        const prNumber = Number(pick(deps.pr, "prNumber"));
        const prUrl = asStr(deps.pr, "prUrl");

        // Delete the enumerated tracked boilerplate (git rm also removes from disk).
        const deletedTracked: string[] = [];
        for (const file of BOILERPLATE_TRACKED_FILES) {
          if (trySh("git", ["ls-files", "--error-unmatch", file]) !== null) {
            sh("git", ["rm", "-q", "--", file]);
            deletedTracked.push(file);
          }
        }
        // Delete the enumerated untracked artifact directory (research/design/plan
        // artifacts and the PR body file live there; all consumed by now).
        const deletedUntracked: string[] = [];
        for (const rel of BOILERPLATE_UNTRACKED_PATHS) {
          const abs = resolve(REPO_ROOT, rel);
          if (existsSync(abs)) {
            rmSync(abs, { recursive: true, force: true });
            deletedUntracked.push(rel);
          }
        }

        // Commit the deletion (pathspec-scoped) as the branch's final commit, then push.
        const staged = sh("git", ["diff", "--cached", "--name-only"])
          .split("\n")
          .filter(Boolean);
        if (staged.length > 0) {
          const unexpected = staged.filter((f) => !(BOILERPLATE_TRACKED_FILES as readonly string[]).includes(f));
          if (unexpected.length > 0) {
            throw new Error(`Refusing to commit cleanup: unexpected staged files ${unexpected.join(", ")}`);
          }
          sh("git", [
            "commit",
            "-m",
            "chore: remove temporary smithers workflow scaffolding",
            "--",
            ...BOILERPLATE_TRACKED_FILES,
          ]);
        }
        const cleanupSha = sh("git", ["rev-parse", "HEAD"]).trim();
        sh("git", ["push", "fork", `${input.featureBranch}:${input.featureBranch}`]);

        // Verify the draft PR now includes the cleanup: head sha matches and it is
        // still a draft.
        let prHeadSha = "";
        let prStillDraft = false;
        for (let attempt = 0; attempt < 8; attempt++) {
          const view = JSON.parse(
            sh("gh", [
              "pr",
              "view",
              String(prNumber),
              "--repo",
              input.upstreamRepo,
              "--json",
              "headRefOid,isDraft",
            ]),
          ) as { headRefOid: string; isDraft: boolean };
          prHeadSha = view.headRefOid;
          prStillDraft = view.isDraft;
          if (prHeadSha === cleanupSha) break;
          await sleep(7_500);
        }
        if (prHeadSha !== cleanupSha) {
          throw new Error(`PR #${prNumber} head (${prHeadSha}) never caught up to the cleanup commit (${cleanupSha}).`);
        }
        // Scaffolding check against the local tree at the verified head commit:
        // `gh pr view --json files` truncates on large PRs, so a complete
        // `git ls-tree` of the exact commit the PR now points at is the only
        // non-truncating way to prove no .smithers/ path ships in the PR.
        const smithersLeftovers = sh("git", ["ls-tree", "-r", "--name-only", cleanupSha])
          .split("\n")
          .map((f) => f.trim())
          .filter((f) => f.startsWith(".smithers/"));
        if (smithersLeftovers.length > 0) {
          throw new Error(`PR #${prNumber} still contains scaffolding paths after cleanup: ${smithersLeftovers.join(", ")}`);
        }
        if (!prStillDraft) {
          throw new Error(`PR #${prNumber} is no longer a draft; it must remain a draft.`);
        }
        return {
          deletedTracked,
          deletedUntracked,
          cleanupSha,
          prNumber,
          prUrl,
          prHeadSha,
          prIncludesCleanup: true,
          prStillDraft,
          notes: RUNTIME_RESIDUE_NOTE,
        };
      }}
    </Task>
  );
}
