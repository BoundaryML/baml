// Deterministic series verification; noRetry because every check is a pure
// function of local git state (infinite default retries would spin on a
// deterministic failure instead of failing the run).
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { asStr, sh } from "../lib/helpers.ts";

export const seriesCheckSchema = z.object({
  ok: z.literal(true),
  baseSha: z.string().min(7),
  headSha: z.string().min(7),
  commitCount: z.number().int().min(2),
});

type VerifySeriesProps = {
  outputs: any;
  input: { baseBranch: string };
};

export function VerifySeries({ outputs, input }: VerifySeriesProps) {
  return (
    <Task
      id="verify-series"
      output={outputs.bttSeriesCheck}
      needs={{ series: "commit-series", preHead: "record-pre-series-head" }}
      deps={{ series: outputs.bttCommits, preHead: outputs.bttPreSeries }}
      timeoutMs={10 * 60_000}
      noRetry
    >
      {(deps: any) => {
        const baseSha = sh("git", ["rev-parse", `origin/${input.baseBranch}`]).trim();
        const headSha = sh("git", ["rev-parse", "HEAD"]).trim();
        const shas = sh("git", ["rev-list", "--reverse", `${baseSha}..HEAD`])
          .trim()
          .split("\n")
          .filter(Boolean);
        if (shas.length < 2) throw new Error(`Commit series too short (${shas.length} commits after base).`);
        const filesOf = (sha: string) =>
          sh("git", ["show", "--name-only", "--format=", sha])
            .split("\n")
            .map((f) => f.trim())
            .filter(Boolean);
        const first = filesOf(shas[0] as string);
        if (first.length === 0 || !first.every((f) => f.startsWith(".smithers/"))) {
          throw new Error(`First commit ${shas[0]} must touch only .smithers/** scaffolding files; saw: ${first.join(", ")}`);
        }
        for (const sha of shas.slice(1)) {
          const offenders = filesOf(sha).filter((f) => f.startsWith(".smithers/"));
          if (offenders.length > 0) {
            throw new Error(`Commit ${sha} touches .smithers/** (${offenders.join(", ")}); only the scaffolding and cleanup commits may.`);
          }
        }
        // The rewrite must be pure history reorganization: the end-state tree
        // must byte-identically match the recorded pre-series head.
        const preHeadSha = asStr(deps.preHead, "headSha");
        if (!preHeadSha) throw new Error("Pre-series head was not recorded; cannot verify tree identity.");
        const treeDiff = sh("git", ["diff", "--name-only", preHeadSha, "HEAD"]).trim();
        if (treeDiff !== "") {
          throw new Error(
            `Commit-series rewrite changed content. Files differing from pre-series head ${preHeadSha.slice(0, 12)}:\n${treeDiff}`,
          );
        }
        return { ok: true as const, baseSha, headSha, commitCount: shas.length };
      }}
    </Task>
  );
}
