// Task 2. Research: Codex Sol independently maps current BAML.
/** @jsxImportSource smthrs */
import { Task } from "smthrs";
import { codexSol } from "../lib/agents.ts";
import { ARTIFACT_DIR_REL, HEARTBEAT_MS, REPO_ROOT, RESEARCH_TIMEOUT_MS } from "../lib/constants.ts";
import { NO_TEVM_RULE } from "../lib/prompt-fragments.ts";
import ResearchBamlPrompt from "../prompts/research-baml.mdx";
import { researchSchema } from "./research-tevm.tsx";

export const bamlResearchSchema = researchSchema.extend({});

export function ResearchBaml({ outputs }: { outputs: any }) {
  return (
    <Task
      id="research-baml"
      output={outputs.bttBamlResearch}
      agent={codexSol}
      needs={{ pre: "preflight" }}
      deps={{ pre: outputs.bttPreflight }}
      timeoutMs={RESEARCH_TIMEOUT_MS}
      heartbeatTimeoutMs={HEARTBEAT_MS}
      retries={1}
    >
      {() => <ResearchBamlPrompt repoRoot={REPO_ROOT} noTevmRule={NO_TEVM_RULE()} artifactDirRel={ARTIFACT_DIR_REL} />}
    </Task>
  );
}
