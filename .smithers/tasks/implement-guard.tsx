// Guard between implementation and review: an output row exists only when the
// implementation (legacy single node, or all planned lane commits + landing)
// actually completed, so downstream deps carry ordering in the graph itself.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { asArr, asStr } from "../lib/helpers.ts";
import type { CommitPlanEntry } from "../lib/plan.ts";

export const guardSchema = z.object({
  ok: z.literal(true),
  note: z.string().default(""),
});

// LEGACY (resumed first run): guards the single `implement` node's report.
export function LegacyImplementGuard({ outputs }: { outputs: any }) {
  return (
    <Task
      id="implement-guard"
      output={outputs.bttImplGuard}
      needs={{ impl: "implement" }}
      deps={{ impl: outputs.bttImpl }}
      timeoutMs={5 * 60_000}
      noRetry
    >
      {(deps: any) => {
        if (asStr(deps.impl, "status") !== "complete") {
          throw new Error(
            `Implementation reported status "${asStr(deps.impl, "status")}" (reason: ${asStr(deps.impl, "blockedReason") || "none given"}). Refusing to proceed to review.`,
          );
        }
        return { ok: true as const, note: `implementation complete: ${asArr(deps.impl, "filesChanged").length} files reported` };
      }}
    </Task>
  );
}

// FRESH: guards the merge-queue landing plus every planned lane commit row.
type LaneGuardProps = {
  outputs: any;
  ctx: any;
  commitPlan: CommitPlanEntry[];
  featureBranch: string;
};

export function LaneImplementGuard({ outputs, ctx, commitPlan, featureBranch }: LaneGuardProps) {
  return (
    <Task
      id="implement-guard"
      output={outputs.bttImplGuard}
      needs={{ land: "merge-lanes" }}
      deps={{ land: outputs.bttLand }}
      timeoutMs={5 * 60_000}
      noRetry
    >
      {(deps: any) => {
        if (asStr(deps.land, "status") !== "complete") {
          throw new Error(
            `Lane landing reported status "${asStr(deps.land, "status")}" (${asStr(deps.land, "blockedReason") || "no reason"}).`,
          );
        }
        for (let i = 0; i < commitPlan.length; i++) {
          const rowData = ctx.outputMaybe(outputs.bttLane, { nodeId: `commit-${i}` });
          if (!rowData || asStr(rowData, "status") !== "complete") {
            throw new Error(`Planned commit ${i} (${commitPlan[i]?.subject}) is not complete; refusing to proceed.`);
          }
        }
        return { ok: true as const, note: `${commitPlan.length} planned commits landed on ${featureBranch}` };
      }}
    </Task>
  );
}
