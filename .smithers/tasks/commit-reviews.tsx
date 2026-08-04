// Task 7d. Per-commit Sol review with an easy-to-review artifact per commit
// (user requirement). Runs on the verified clean series.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { codexSol } from "../lib/agents.ts";
import { ARTIFACT_DIR_REL, HEARTBEAT_MS, REVIEW_TIMEOUT_MS } from "../lib/constants.ts";
import { planContext } from "../lib/plan.ts";
import { NO_TEVM_RULE } from "../lib/prompt-fragments.ts";
import CommitReviewsPrompt from "../prompts/commit-reviews.mdx";

export const commitReviewsSchema = z.object({
  artifactDir: z.string().min(1),
  reviewedCommits: z.array(z.string()).min(1),
  verdict: z.enum(["clean", "issues-found"]),
  issues: z.array(z.string()).default([]),
  summary: z.string().min(50),
});

type CommitReviewsProps = {
  outputs: any;
  input: { baseBranch: string };
};

export function CommitReviews({ outputs, input }: CommitReviewsProps) {
  return (
    <Task
      id="commit-reviews"
      output={outputs.bttCommitReviews}
      agent={codexSol}
      needs={{ check: "verify-series", plan: "synthesize-plan" }}
      deps={{ check: outputs.bttSeriesCheck, plan: outputs.bttPlan }}
      timeoutMs={REVIEW_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {(deps: any) => (
        <CommitReviewsPrompt
          baseBranch={input.baseBranch}
          planContext={planContext(deps.plan)}
          artifactDirRel={ARTIFACT_DIR_REL}
          noTevmRule={NO_TEVM_RULE()}
        />
      )}
    </Task>
  );
}
