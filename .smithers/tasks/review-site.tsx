// Task 7f. Commit-by-commit review site deployed to baml-unplugin-pr.smithers.sh
// (user requirement): one slide per atomic emoji commit — diff, concise
// explanation, diagrams — composed from the smithers UI components.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { codexSolBuilder } from "../lib/agents.ts";
import { ARTIFACT_DIR_REL, HEARTBEAT_MS, IMPLEMENT_TIMEOUT_MS } from "../lib/constants.ts";
import { asStr } from "../lib/helpers.ts";
import { planContext } from "../lib/plan.ts";
import SiteDeployPrompt from "../prompts/site-deploy.mdx";

export const siteSchema = z.object({
  url: z.string().min(10),
  deployedCommit: z.string().min(7),
  slideCount: z.number().int().min(2),
  summary: z.string().min(50),
});

type ReviewSiteProps = {
  outputs: any;
  input: { upstreamRepo: string; baseBranch: string };
  // Fresh mode: the LAST stack PR's node/table; legacy mode: create-draft-pr/bttPr.
  prNodeId: string;
  prOutput: any;
};

export function ReviewSite({ outputs, input, prNodeId, prOutput }: ReviewSiteProps) {
  return (
    <Task
      id="review-site"
      output={outputs.bttSite}
      agent={codexSolBuilder}
      needs={{ pr: prNodeId, reviews: "commit-reviews", plan: "synthesize-plan" }}
      deps={{ pr: prOutput, reviews: outputs.bttCommitReviews, plan: outputs.bttPlan }}
      timeoutMs={IMPLEMENT_TIMEOUT_MS / 4}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {(deps: any) => (
        <SiteDeployPrompt
          planContext={planContext(deps.plan)}
          baseBranch={input.baseBranch}
          artifactDirRel={ARTIFACT_DIR_REL}
          prUrl={asStr(deps.pr, "prUrl")}
        />
      )}
    </Task>
  );
}
