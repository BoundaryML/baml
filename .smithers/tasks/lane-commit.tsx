// Task 5 (fresh shape): one atomic planned commit implemented inside its lane's
// isolated worktree, by the commit's cost-tier agent (sol/terra/luna).
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { TIER_AGENTS } from "../lib/agents.ts";
import { FIX_TIMEOUT_MS, HEARTBEAT_MS } from "../lib/constants.ts";
import { type CommitPlanEntry, planContext } from "../lib/plan.ts";
import { NO_SMITHERS_RULE, NO_TEVM_RULE, USER_STEERING } from "../lib/prompt-fragments.ts";
import LaneCommitPrompt from "../prompts/lane-commit.mdx";

// One commit-plan entry implemented inside a lane worktree.
export const laneCommitSchema = z.object({
  status: z.enum(["complete", "blocked"]),
  commitSha: z.string().min(7),
  subject: z.string().min(8),
  validationEvidence: z.string().min(40),
  summary: z.string().min(50),
  blockedReason: z.string().default(""),
});

type LaneCommitTaskProps = {
  outputs: any;
  entry: CommitPlanEntry;
  index: number;
  // The previous commit node on the same lane (sequential dependency), if any.
  prevNodeId?: string;
};

export function LaneCommitTask({ outputs, entry, index, prevNodeId }: LaneCommitTaskProps) {
  return (
    <Task
      id={`commit-${index}`}
      output={outputs.bttLane}
      agent={TIER_AGENTS[entry.tier]}
      needs={{
        plan: "synthesize-plan",
        ...(prevNodeId ? { prev: prevNodeId } : {}),
      }}
      deps={{
        plan: outputs.bttPlan,
        ...(prevNodeId ? { prev: outputs.bttLane } : {}),
      }}
      timeoutMs={FIX_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {(deps: any) => (
        <LaneCommitPrompt
          commitNumber={String(index + 1)}
          lane={String(entry.lane)}
          planContext={planContext(deps.plan)}
          subject={`${entry.emoji} ${entry.subject}`}
          rationale={entry.rationale}
          files={entry.files.join(", ")}
          validation={entry.validation.map((v) => `  - ${v}`).join("\n")}
          noTevmRule={NO_TEVM_RULE()}
          noSmithersRule={NO_SMITHERS_RULE}
          userSteering={USER_STEERING}
        />
      )}
    </Task>
  );
}
