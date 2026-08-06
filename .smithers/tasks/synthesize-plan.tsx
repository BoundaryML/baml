// Task 4. Fable (with Opus failover) synthesizes the designs into the exact final
// plan (incl. the file manifest and the tiered, laned atomic commit plan).
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { fableWithOpusFailover } from "../lib/agents.ts";
import { ARTIFACT_DIR_REL, HEARTBEAT_MS, SYNTH_TIMEOUT_MS } from "../lib/constants.ts";
import { asStr, readArtifact } from "../lib/helpers.ts";
import { FEATURE_BRIEF, NO_SMITHERS_RULE, NO_TEVM_RULE, USER_STEERING } from "../lib/prompt-fragments.ts";
import SynthesisPrompt from "../prompts/synthesis.mdx";

// One atomic emoji commit in the synthesized commit plan (user requirement): every
// commit carries its description, exact file set, validation criteria, a cost tier
// (sol = difficult/important, terra = straightforward, luna = trivial), and a lane
// number so independent lanes can be implemented in parallel worktrees and landed
// through a merge queue.
export const commitPlanEntrySchema = z.object({
  emoji: z.string().min(1),
  subject: z.string().min(8),
  rationale: z.string().min(20),
  files: z.array(z.string()).min(1),
  validation: z.array(z.string()).min(1),
  tier: z.enum(["sol", "terra", "luna"]).default("terra"),
  lane: z.number().int().min(0),
  // Which stacked PR the commit belongs to (1-based, simplest/most standalone
  // PR first; deeply coupled work stacks later on its prerequisites).
  pr: z.number().int().min(1).default(1),
});

export const planSchema = z.object({
  artifactPath: z.string().min(1),
  summary: z.string().min(200),
  packageBoundaries: z.string().min(100),
  languageServiceBehavior: z.string().min(100),
  sourceMapStrategy: z.string().min(50),
  testPlan: z.string().min(100),
  rolloutPlan: z.string().min(50),
  disagreementsResolved: z.array(z.string()).min(1),
  // Structured atomic-commit plan (empty on legacy rows from the first run).
  commitPlan: z.array(commitPlanEntrySchema).default([]),
});

type SynthesizePlanProps = {
  outputs: any;
  upstreamRepo: string;
  kimiSeatActive: boolean;
};

export function SynthesizePlan({ outputs, upstreamRepo, kimiSeatActive }: SynthesizePlanProps) {
  return (
    <Task
      id="synthesize-plan"
      output={outputs.bttPlan}
      agent={fableWithOpusFailover}
      needs={{
        dSol: "design-codex-sol",
        dFable: "design-claude-fable",
        ...(kimiSeatActive ? { dKimi: "design-kimi-opencode" } : {}),
        tevm: "research-tevm",
        baml: "research-baml",
      }}
      deps={{
        dSol: outputs.bttDesign,
        dFable: outputs.bttDesign,
        ...(kimiSeatActive ? { dKimi: outputs.bttDesign } : {}),
        tevm: outputs.bttTevmResearch,
        baml: outputs.bttBamlResearch,
      }}
      timeoutMs={SYNTH_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {(deps: any) => {
        const designs = [deps.dSol, deps.dFable, ...(deps.dKimi ? [deps.dKimi] : [])];
        const blocks = designs
          .map((d) => {
            const seat = asStr(d, "seat", "unknown-seat");
            const p = asStr(d, "artifactPath");
            return `=== DESIGN (${seat}) from ${p} ===\n${readArtifact(p)}\n=== END DESIGN (${seat}) ===`;
          })
          .join("\n\n");
        const bamlPath = asStr(deps.baml, "artifactPath", `${ARTIFACT_DIR_REL}/baml-architecture.md`);
        return (
          <SynthesisPrompt
            featureBrief={FEATURE_BRIEF}
            noTevmRule={NO_TEVM_RULE()}
            bamlPath={bamlPath}
            smithersRule={NO_SMITHERS_RULE.replace("Never touch .git internals directly; use normal git commands only where instructed.", "")}
            designBlocks={blocks}
            userSteering={USER_STEERING}
            upstreamRepo={upstreamRepo}
            artifactDirRel={ARTIFACT_DIR_REL}
          />
        );
      }}
    </Task>
  );
}
