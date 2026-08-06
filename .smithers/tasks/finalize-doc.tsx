// Task 6b. Fable (Opus failover) owns and finalizes the one-page maintainer
// architecture doc, and drafts the PR title/body. Mounted by the workflow ONLY
// once the review loop produced an explicit approval with passing tests.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { fableWithOpusFailover } from "../lib/agents.ts";
import { ARTIFACT_DIR_REL, DOC_TIMEOUT_MS, HEARTBEAT_MS } from "../lib/constants.ts";
import { asStr, unwrap } from "../lib/helpers.ts";
import { NO_SMITHERS_RULE, NO_TEVM_RULE } from "../lib/prompt-fragments.ts";
import FinalizeDocPrompt from "../prompts/finalize-doc.mdx";

export const docSchema = z.object({
  docPath: z.string().min(1),
  prTitle: z.string().min(10),
  prBody: z.string().min(200),
  summary: z.string().min(50),
});

type FinalizeDocProps = {
  outputs: any;
  ctx: any;
  input: { upstreamRepo: string; baseBranch: string; featureBranch: string };
};

export function FinalizeDoc({ outputs, ctx, input }: FinalizeDocProps) {
  return (
    <Task
      id="finalize-doc"
      output={outputs.bttDoc}
      agent={fableWithOpusFailover}
      timeoutMs={DOC_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {() => {
        const impl = unwrap(ctx.latest(outputs.bttImpl, "implement"));
        const plan = unwrap(ctx.latest(outputs.bttPlan, "synthesize-plan"));
        return (
          <FinalizeDocPrompt
            docDraftPath={asStr(impl, "docDraftPath", "(path in the plan)")}
            planPath={asStr(plan, "artifactPath", `${ARTIFACT_DIR_REL}/final-plan.md`)}
            featureBranch={input.featureBranch}
            upstreamRepo={input.upstreamRepo}
            baseBranch={input.baseBranch}
            noTevmRule={NO_TEVM_RULE()}
            noSmithersRule={NO_SMITHERS_RULE}
          />
        );
      }}
    </Task>
  );
}
