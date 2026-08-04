// Task 5 (LEGACY shape, resumed first run only): the original single 8-hour
// implement node. Fresh runs use the commit-plan lanes instead (retro change 4).
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { codexSolBuilder } from "../lib/agents.ts";
import { ARTIFACT_DIR_REL, HEARTBEAT_MS, IMPLEMENT_TIMEOUT_MS } from "../lib/constants.ts";
import { asStr, readArtifact } from "../lib/helpers.ts";
import { NO_SMITHERS_RULE, NO_TEVM_RULE, USER_STEERING } from "../lib/prompt-fragments.ts";
import ImplementPrompt from "../prompts/implement.mdx";

export const implSchema = z.object({
  status: z.enum(["complete", "blocked"]),
  summary: z.string().min(200),
  filesChanged: z.array(z.string()).min(5),
  testCommands: z.array(z.string()).min(1),
  docDraftPath: z.string().min(1),
  blockedReason: z.string().default(""),
});

export function Implement({ outputs }: { outputs: any }) {
  return (
    <Task
      id="implement"
      output={outputs.bttImpl}
      agent={codexSolBuilder}
      needs={{ plan: "synthesize-plan" }}
      deps={{ plan: outputs.bttPlan }}
      timeoutMs={IMPLEMENT_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {(deps: any) => {
        const planPath = asStr(deps.plan, "artifactPath", `${ARTIFACT_DIR_REL}/final-plan.md`);
        return (
          <ImplementPrompt
            planPath={planPath}
            planArtifact={readArtifact(planPath)}
            noTevmRule={NO_TEVM_RULE()}
            noSmithersRule={NO_SMITHERS_RULE}
            userSteering={USER_STEERING}
          />
        );
      }}
    </Task>
  );
}
