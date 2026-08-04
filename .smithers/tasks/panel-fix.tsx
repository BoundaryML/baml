// Task 7g (loop body). Codex Sol fixes everything the sign-off panel raised;
// keyed off the moderator verdict (allLgtm=false) plus each panelist's issues.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { codexSolBuilder } from "../lib/agents.ts";
import { FIX_TIMEOUT_MS, HEARTBEAT_MS } from "../lib/constants.ts";
import { asArr, asStr, unwrap } from "../lib/helpers.ts";
import { NO_SMITHERS_RULE, NO_TEVM_RULE, USER_STEERING } from "../lib/prompt-fragments.ts";
import PanelFixPrompt from "../prompts/panel-fix.mdx";

export const panelFixSchema = z.object({
  summary: z.string().min(50),
  addressed: z.array(z.string()).min(1),
});

export function PanelFix({ outputs, ctx }: { outputs: any; ctx: any }) {
  return (
    <Task
      id="panel-fix"
      output={outputs.bttPanelFix}
      agent={codexSolBuilder}
      timeoutMs={FIX_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {() => {
        const seats = ["signoff-kimi", "signoff-sol", "signoff-fable", "signoff-opus"] as const;
        const issues = seats
          .flatMap((nodeId) => {
            const rowData = ctx.outputMaybe(outputs.bttPanel, { nodeId });
            return asArr(rowData, "issues").map((i) => {
              const r = unwrap(i);
              return `- [${asStr(r, "severity", "major")}] (${asStr(rowData, "seat", nodeId)}) ${asStr(r, "title")}: ${asStr(r, "detail")}`;
            });
          })
          .join("\n");
        const verdict = ctx.outputMaybe(outputs.bttPanelVerdict, { nodeId: "signoff-moderator" });
        const outstanding = asArr(verdict, "outstanding")
          .map((o) => `- ${String(o)}`)
          .join("\n");
        return (
          <PanelFixPrompt
            outstanding={outstanding || "- (the moderator listed no structured outstanding items; read its summary in the run outputs)"}
            issues={issues || "- (a seat withheld LGTM without structured issues; read the panel notes in the run outputs and address them)"}
            noTevmRule={NO_TEVM_RULE()}
            noSmithersRule={NO_SMITHERS_RULE}
            userSteering={USER_STEERING}
          />
        );
      }}
    </Task>
  );
}
