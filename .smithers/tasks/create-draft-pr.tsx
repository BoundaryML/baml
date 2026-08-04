// Task 7c. Push to the user's fork and open (or verify) the DRAFT PR. Never non-draft.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { z } from "zod/v4";
import { ARTIFACT_DIR } from "../lib/constants.ts";
import { asStr, sh } from "../lib/helpers.ts";

export const prSchema = z.object({
  prNumber: z.number().int().min(1),
  prUrl: z.string().min(10),
  headSha: z.string().min(7),
  draft: z.literal(true),
});

type CreateDraftPrProps = {
  outputs: any;
  input: { upstreamRepo: string; baseBranch: string; featureBranch: string };
};

export function CreateDraftPr({ outputs, input }: CreateDraftPrProps) {
  return (
    <Task
      id="create-draft-pr"
      output={outputs.bttPr}
      sideEffect={{ idempotent: true }}
      needs={{ pre: "preflight", doc: "finalize-doc", check: "verify-series" }}
      deps={{ pre: outputs.bttPreflight, doc: outputs.bttDoc, check: outputs.bttSeriesCheck }}
      timeoutMs={15 * 60_000}
      retries={2}
      retryPolicy={{ backoff: "exponential", initialDelayMs: 10_000 }}
    >
      {async (deps: any) => {
        const login = asStr(deps.pre, "login");
        const head = `${login}:${input.featureBranch}`;
        sh("git", ["push", "--force", "fork", `${input.featureBranch}:${input.featureBranch}`]);
        const list = JSON.parse(
          sh("gh", [
            "pr",
            "list",
            "--repo",
            input.upstreamRepo,
            "--head",
            head,
            "--state",
            "open",
            "--json",
            "number,isDraft,url",
          ]) || "[]",
        ) as Array<{ number: number; isDraft: boolean; url: string }>;
        let pr = list[0];
        if (!pr) {
          const bodyFile = join(ARTIFACT_DIR, "pr-body.md");
          writeFileSync(bodyFile, asStr(deps.doc, "prBody"), "utf8");
          sh("gh", [
            "pr",
            "create",
            "--repo",
            input.upstreamRepo,
            "--base",
            input.baseBranch,
            "--head",
            head,
            "--draft",
            "--title",
            asStr(deps.doc, "prTitle"),
            "--body-file",
            bodyFile,
          ]);
          const relisted = JSON.parse(
            sh("gh", ["pr", "list", "--repo", input.upstreamRepo, "--head", head, "--state", "open", "--json", "number,isDraft,url"]),
          ) as Array<{ number: number; isDraft: boolean; url: string }>;
          pr = relisted[0];
        }
        if (!pr) throw new Error("Draft PR creation appeared to succeed but the PR could not be found.");
        if (!pr.isDraft) {
          throw new Error(
            `PR #${pr.number} (${pr.url}) is NOT a draft. This workflow must never produce a non-draft PR; convert it back to draft manually and retry this task.`,
          );
        }
        const headSha = sh("git", ["rev-parse", "HEAD"]).trim();
        return { prNumber: pr.number, prUrl: pr.url, headSha, draft: true as const };
      }}
    </Task>
  );
}
