// Task 1. Research: Codex Sol maps TEVM into a self-contained artifact.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { z } from "zod/v4";
import { codexSol } from "../lib/agents.ts";
import { ARTIFACT_DIR_REL, HEARTBEAT_MS, RESEARCH_TIMEOUT_MS, tevmRepoPath } from "../lib/constants.ts";
import ResearchTevmPrompt from "../prompts/research-tevm.mdx";

export const researchSchema = z.object({
  artifactPath: z.string().min(1),
  summary: z.string().min(200),
  sectionsCovered: z.array(z.string()).min(6),
});

export function ResearchTevm({ outputs }: { outputs: any }) {
  return (
    <Task
      id="research-tevm"
      output={outputs.bttTevmResearch}
      agent={codexSol}
      needs={{ pre: "preflight" }}
      deps={{ pre: outputs.bttPreflight }}
      timeoutMs={RESEARCH_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {() => <ResearchTevmPrompt tevmRepo={tevmRepoPath()} artifactDirRel={ARTIFACT_DIR_REL} />}
    </Task>
  );
}
