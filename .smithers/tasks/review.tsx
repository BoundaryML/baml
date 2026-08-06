// Task 6. The reviewing authority (Fable, Opus failover) inside the bounded
// review loop; runs the tests itself and holds an upstream-quality bar.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { fableWithOpusFailover } from "../lib/agents.ts";
import { ARTIFACT_DIR_REL, HEARTBEAT_MS, REVIEW_TIMEOUT_MS } from "../lib/constants.ts";
import { asArr, asStr, unwrap } from "../lib/helpers.ts";
import { NO_SMITHERS_RULE, NO_TEVM_RULE, USER_STEERING } from "../lib/prompt-fragments.ts";
import ReviewPrompt from "../prompts/review.mdx";

export const reviewSchema = z.object({
  approved: z.boolean(),
  testsPassed: z.boolean(),
  testEvidence: z.string().min(40),
  blockingIssues: z
    .array(
      z.object({
        severity: z.enum(["critical", "major", "minor"]).default("major"),
        title: z.string().min(1),
        detail: z.string().default(""),
      }),
    )
    .default([]),
  feedback: z.string().min(20),
});

type ReviewProps = {
  outputs: any;
  ctx: any;
  input: { maxReviewRounds: number; baseBranch: string; upstreamRepo: string };
};

export function Review({ outputs, ctx, input }: ReviewProps) {
  return (
    <Task
      id="review"
      output={outputs.bttReview}
      agent={fableWithOpusFailover}
      needs={{ guard: "implement-guard", gate: "ci-gate" }}
      deps={{ guard: outputs.bttImplGuard, gate: outputs.bttCiGate }}
      timeoutMs={REVIEW_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {() => {
        const impl = unwrap(ctx.latest(outputs.bttImpl, "implement"));
        const plan = unwrap(ctx.latest(outputs.bttPlan, "synthesize-plan"));
        const lastFix = ctx.latest(outputs.bttFix, "fix");
        const gate = ctx.latest(outputs.bttCiGate, "ci-gate");
        const gateNote = gate
          ? `A deterministic CI gate already passed these mechanical checks — do NOT spend review time re-running them; focus on semantics, tests, and the design principles:\n${asArr(gate, "checks").map((c) => `- ${String(c)}`).join("\n")}`
          : "";
        const testCommands = [
          ...asArr(impl, "testCommands").map((c) => `- ${String(c)}`),
          ...(asStr(plan, "testPlan") ? [`- (plan test plan) ${asStr(plan, "testPlan").slice(0, 600)}`] : []),
        ].join("\n");
        const planPath = asStr(plan, "artifactPath", `${ARTIFACT_DIR_REL}/final-plan.md`);
        const fixNote = lastFix ? "A fix round has run since the last review; re-review everything, not just the fix.\n\n" : "";
        return (
          <ReviewPrompt
            maxReviewRounds={String(input.maxReviewRounds)}
            fixNote={fixNote}
            gateNote={gateNote}
            planPath={planPath}
            baseBranch={input.baseBranch}
            testCommands={testCommands || "- (none declared; derive the right commands yourself)"}
            noTevmRule={NO_TEVM_RULE()}
            noSmithersRule={NO_SMITHERS_RULE}
            upstreamRepo={input.upstreamRepo}
            userSteering={USER_STEERING}
          />
        );
      }}
    </Task>
  );
}
