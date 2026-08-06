// Task 3. One seat of the independent design panel; every seat sees ONLY the two
// research artifacts (embedded verbatim into each prompt).
/** @jsxImportSource smthrs */
import { Task, type AgentLike } from "smthrs";
import { z } from "zod/v4";
import { ARTIFACT_DIR_REL, DESIGN_TIMEOUT_MS, HEARTBEAT_MS } from "../lib/constants.ts";
import { asStr, readArtifact } from "../lib/helpers.ts";
import { FEATURE_BRIEF, NO_TEVM_RULE, USER_STEERING } from "../lib/prompt-fragments.ts";
import DesignPrompt from "../prompts/design.mdx";

export const designSchema = z.object({
  seat: z.enum(["codex-sol", "claude-fable", "kimi-opencode"]),
  artifactPath: z.string().min(1),
  summary: z.string().min(200),
  keyDecisions: z.array(z.string()).min(3),
  risks: z.array(z.string()).default([]),
});

export type DesignSeat = "codex-sol" | "claude-fable" | "kimi-opencode";

type DesignSeatTaskProps = {
  outputs: any;
  seat: DesignSeat;
  agent: AgentLike | AgentLike[];
};

export function DesignSeatTask({ outputs, seat, agent }: DesignSeatTaskProps) {
  return (
    <Task
      id={`design-${seat}`}
      output={outputs.bttDesign}
      agent={agent}
      needs={{ tevm: "research-tevm", baml: "research-baml" }}
      deps={{ tevm: outputs.bttTevmResearch, baml: outputs.bttBamlResearch }}
      timeoutMs={DESIGN_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {(deps: any) => {
        const tevmPath = asStr(deps.tevm, "artifactPath", `${ARTIFACT_DIR_REL}/tevm-architecture.md`);
        const bamlPath = asStr(deps.baml, "artifactPath", `${ARTIFACT_DIR_REL}/baml-architecture.md`);
        return (
          <DesignPrompt
            seat={seat}
            featureBrief={FEATURE_BRIEF}
            noTevmRule={NO_TEVM_RULE()}
            tevmPath={tevmPath}
            tevmArtifact={readArtifact(tevmPath)}
            bamlPath={bamlPath}
            bamlArtifact={readArtifact(bamlPath)}
            userSteering={USER_STEERING}
            artifactDirRel={ARTIFACT_DIR_REL}
          />
        );
      }}
    </Task>
  );
}
