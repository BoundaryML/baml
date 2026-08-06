// Task 5 (fresh shape): merge-queue task landing the completed lane branches
// onto the feature branch in lane order.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { codexSolBuilder } from "../lib/agents.ts";
import { HEARTBEAT_MS, SERIES_TIMEOUT_MS } from "../lib/constants.ts";
import { type CommitPlanEntry, planContext } from "../lib/plan.ts";
import { NO_SMITHERS_RULE } from "../lib/prompt-fragments.ts";
import LandLanesPrompt from "../prompts/land-lanes.mdx";

export const landSchema = z.object({
  status: z.enum(["complete", "blocked"]),
  landedLanes: z.array(z.string()).min(1),
  headSha: z.string().min(7),
  summary: z.string().min(50),
  blockedReason: z.string().default(""),
});

type MergeLanesProps = {
  outputs: any;
  commitPlan: CommitPlanEntry[];
  lanes: number[];
  lastLaneNodeOf: (lane: number) => string;
  featureBranch: string;
};

export function MergeLanes({ outputs, commitPlan, lanes, lastLaneNodeOf, featureBranch }: MergeLanesProps) {
  return (
    <Task
      id="merge-lanes"
      output={outputs.bttLand}
      agent={codexSolBuilder}
      needs={Object.fromEntries([
        ["plan", "synthesize-plan"],
        ...lanes.map((lane) => [`lane${lane}`, lastLaneNodeOf(lane)]),
      ])}
      deps={Object.fromEntries([
        ["plan", outputs.bttPlan],
        ...lanes.map((lane) => [`lane${lane}`, outputs.bttLane]),
      ])}
      timeoutMs={SERIES_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {(deps: any) => {
        const laneIds = [...new Set(commitPlan.map((e) => e.lane))].sort((a, b) => a - b);
        return (
          <LandLanesPrompt
            featureBranch={featureBranch}
            planContext={planContext(deps.plan)}
            lanesList={laneIds.map((l) => `btt-lane-${l} (worktree .smithers/worktrees/lane-${l})`).join(", ")}
            noSmithersRule={NO_SMITHERS_RULE}
          />
        );
      }}
    </Task>
  );
}
