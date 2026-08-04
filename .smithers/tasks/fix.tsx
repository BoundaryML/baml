// Task 6 (loop body). Codex fixes every blocking issue from the last review.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { codexSolBuilder } from "../lib/agents.ts";
import { FIX_TIMEOUT_MS, HEARTBEAT_MS } from "../lib/constants.ts";
import { asArr, asStr, unwrap } from "../lib/helpers.ts";
import { NO_SMITHERS_RULE, NO_TEVM_RULE, USER_STEERING } from "../lib/prompt-fragments.ts";
import FixPrompt from "../prompts/fix.mdx";

export const fixSchema = z.object({
  summary: z.string().min(50),
  addressed: z.array(z.string()).min(1),
});

export function Fix({ outputs, ctx }: { outputs: any; ctx: any }) {
  return (
    <Task
      id="fix"
      output={outputs.bttFix}
      agent={codexSolBuilder}
      timeoutMs={FIX_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {() => {
        const review = unwrap(ctx.latest(outputs.bttReview, "review"));
        const issues = asArr(review, "blockingIssues")
          .map((i) => {
            const r = unwrap(i);
            return `- [${asStr(r, "severity", "major")}] ${asStr(r, "title")}: ${asStr(r, "detail")}`;
          })
          .join("\n");
        return (
          <FixPrompt
            feedback={asStr(review, "feedback", "(no feedback text)")}
            issues={issues || "- (reviewer reported failing tests; make them pass)"}
            testEvidence={asStr(review, "testEvidence", "(none)")}
            noTevmRule={NO_TEVM_RULE()}
            noSmithersRule={NO_SMITHERS_RULE}
            userSteering={USER_STEERING}
          />
        );
      }}
    </Task>
  );
}
