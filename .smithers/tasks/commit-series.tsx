// Task 7b. Clean commit series (scaffolding commit stays first and untouched).
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { codexSolBuilder } from "../lib/agents.ts";
import { HEARTBEAT_MS, SERIES_TIMEOUT_MS } from "../lib/constants.ts";
import { asStr, unwrap } from "../lib/helpers.ts";
import { NO_SMITHERS_RULE } from "../lib/prompt-fragments.ts";
import CommitSeriesPrompt from "../prompts/commit-series.mdx";

export const commitsSchema = z.object({
  commitCount: z.number().int().min(2),
  subjects: z.array(z.string()).min(2),
  notes: z.string().default(""),
});

type CommitSeriesProps = {
  outputs: any;
  ctx: any;
  input: { baseBranch: string };
};

export function CommitSeries({ outputs, ctx, input }: CommitSeriesProps) {
  return (
    <Task
      id="commit-series"
      output={outputs.bttCommits}
      agent={codexSolBuilder}
      needs={{ doc: "finalize-doc", preHead: "record-pre-series-head" }}
      deps={{ doc: outputs.bttDoc, preHead: outputs.bttPreSeries }}
      timeoutMs={SERIES_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {() => {
        const pre = unwrap(ctx.latest(outputs.bttPreflight, "preflight"));
        const preSeriesHead = asStr(ctx.latest(outputs.bttPreSeries, "record-pre-series-head"), "headSha");
        return (
          <CommitSeriesPrompt
            baseBranch={input.baseBranch}
            scaffoldCommit={asStr(pre, "scaffoldCommit").slice(0, 12)}
            preSeriesHead={preSeriesHead ? preSeriesHead.slice(0, 12) : "(recorded by the previous step)"}
            preSeriesHeadRef={preSeriesHead || "<pre-series-head>"}
            noSmithersRule={NO_SMITHERS_RULE}
          />
        );
      }}
    </Task>
  );
}
