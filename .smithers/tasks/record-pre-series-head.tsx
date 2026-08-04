// Task 7a. Record the exact pre-rewrite head (the doc commit), durably, so the
// history rewrite can be verified content-identical afterwards.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { sh } from "../lib/helpers.ts";

export const preSeriesSchema = z.object({
  headSha: z.string().min(7),
});

export function RecordPreSeriesHead({ outputs }: { outputs: any }) {
  return (
    <Task
      id="record-pre-series-head"
      output={outputs.bttPreSeries}
      needs={{ doc: "finalize-doc" }}
      deps={{ doc: outputs.bttDoc }}
      timeoutMs={5 * 60_000}
      noRetry
    >
      {() => ({ headSha: sh("git", ["rev-parse", "HEAD"]).trim() })}
    </Task>
  );
}
