// Commit-plan readers shared by the lane, merge, and review task modules.
// Workflow scaffolding; removed by the final cleanup task.
import type { Tier } from "./agents.ts";
import { ARTIFACT_DIR_REL } from "./constants.ts";
import { asArr, asStr, pick, readArtifact, unwrap } from "./helpers.ts";

// Embeddable plan context (user requirement: the plan file — especially its file
// manifest — rides along in every prompt where knowing the files is useful).
export function planContext(planRow: unknown): string {
  const planPath = asStr(planRow, "artifactPath", `${ARTIFACT_DIR_REL}/final-plan.md`);
  return `=== FINAL PLAN (${planPath}) — includes the File manifest and Commit plan; treat the manifest as the authoritative list of files this effort touches ===\n${readArtifact(planPath)}\n=== END FINAL PLAN ===`;
}

export type CommitPlanEntry = {
  emoji: string;
  subject: string;
  rationale: string;
  files: string[];
  validation: string[];
  tier: Tier;
  lane: number;
};
export function commitPlanOf(planRow: unknown): CommitPlanEntry[] {
  return asArr(planRow, "commitPlan").map((raw) => {
    const r = unwrap(raw);
    const tier = asStr(r, "tier", "terra");
    return {
      emoji: asStr(r, "emoji", "✨"),
      subject: asStr(r, "subject", "unnamed commit"),
      rationale: asStr(r, "rationale", ""),
      files: asArr(r, "files").map(String),
      validation: asArr(r, "validation").map(String),
      tier: (tier === "sol" || tier === "luna" ? tier : "terra") as Tier,
      lane: Number(pick(r, "lane") ?? 0) || 0,
    };
  });
}
