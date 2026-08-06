// Task 7e. Real terminal proof: VHS recordings of the features working, published
// to an assets branch and embedded in the PR body (user requirement).
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { codexTerra } from "../lib/agents.ts";
import { ARTIFACT_DIR_REL, HEARTBEAT_MS, SERIES_TIMEOUT_MS } from "../lib/constants.ts";
import { asStr } from "../lib/helpers.ts";
import { NO_SMITHERS_RULE } from "../lib/prompt-fragments.ts";
import DemosPrompt from "../prompts/demos.mdx";

export const demosSchema = z.object({
  gifs: z.array(z.string()).min(2),
  assetsBranch: z.string().min(1),
  assetsCommit: z.string().min(7),
  summary: z.string().min(50),
});

type RecordDemosProps = {
  outputs: any;
  // Fresh mode: the LAST stack PR's node/table; legacy mode: create-draft-pr/bttPr.
  prNodeId: string;
  prOutput: any;
};

export function RecordDemos({ outputs, prNodeId, prOutput }: RecordDemosProps) {
  return (
    <Task
      id="record-demos"
      output={outputs.bttDemos}
      agent={codexTerra}
      needs={{ pr: prNodeId, check: "verify-series" }}
      deps={{ pr: prOutput, check: outputs.bttSeriesCheck }}
      timeoutMs={SERIES_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {(deps: any) => (
        <DemosPrompt
          artifactDirRel={ARTIFACT_DIR_REL}
          prUrl={asStr(deps.pr, "prUrl")}
          smithersRule={NO_SMITHERS_RULE.replace("Never modify anything under .smithers/", "You may write only under .smithers/artifacts/ within .smithers/")}
        />
      )}
    </Task>
  );
}
