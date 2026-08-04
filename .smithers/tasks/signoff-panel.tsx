// Task 7g. FINAL SIGN-OFF PANEL (user requirement): Kimi K3, Codex Sol, Claude
// Fable, and Claude Opus each independently verify ALL work and artifacts via the
// smithers <Panel> component; a Fable-fronted moderator (Opus failover) rules with
// strategy "consensus" (minAgree 4) and writes the verdict row that gates cleanup.
// Panelist node ids: signoff-kimi / signoff-sol / signoff-fable / signoff-opus;
// moderator: signoff-moderator.
/** @jsxImportSource smthrs */
import { Panel } from "smthrs";
import { z } from "zod/v4";
import { claudeFable, claudeOpus, codexSol, fableWithOpusFailover, kimiOpenCode } from "../lib/agents.ts";
import { ARTIFACT_DIR_REL, HEARTBEAT_MS, REVIEW_TIMEOUT_MS } from "../lib/constants.ts";
import { NO_SMITHERS_RULE, NO_TEVM_RULE, USER_STEERING } from "../lib/prompt-fragments.ts";
import PanelSeatPrompt from "../prompts/panel-seat.mdx";

// One seat of the final sign-off panel. Panelists share one <Panel> prompt, so
// the seat identifier is a free-form string (the panelist self-identifies).
export const panelSchema = z.object({
  seat: z.string().min(2),
  lgtm: z.boolean(),
  issues: z
    .array(
      z.object({
        severity: z.enum(["critical", "major", "minor"]).default("major"),
        title: z.string().min(1),
        detail: z.string().default(""),
      }),
    )
    .default([]),
  notes: z.string().min(20),
});

// The <Panel> moderator's synthesized verdict over the four seats.
export const panelVerdictSchema = z.object({
  allLgtm: z.boolean(),
  summary: z.string().min(50),
  outstanding: z.array(z.string()).default([]),
});

type SignoffPanelProps = {
  outputs: any;
  refs: { prUrl: string; siteUrl: string; docPath: string };
  input: { baseBranch: string; upstreamRepo: string };
};

export function SignoffPanel({ outputs, refs, input }: SignoffPanelProps) {
  const taskProps = { timeoutMs: REVIEW_TIMEOUT_MS, heartbeatTimeoutMs: HEARTBEAT_MS, retries: 1 };
  return (
    <Panel
      id="signoff"
      panelists={[
        { agent: kimiOpenCode, label: "kimi", role: "kimi-k3" },
        { agent: codexSol, label: "sol", role: "codex-sol" },
        { agent: claudeFable, label: "fable", role: "claude-fable" },
        { agent: claudeOpus, label: "opus", role: "claude-opus" },
      ]}
      moderator={fableWithOpusFailover}
      strategy="consensus"
      minAgree={4}
      maxConcurrency={4}
      panelistOutput={outputs.bttPanel}
      moderatorOutput={outputs.bttPanelVerdict}
      panelistTaskProps={taskProps}
      moderatorTaskProps={taskProps}
    >
      <PanelSeatPrompt
        baseBranch={input.baseBranch}
        prUrl={refs.prUrl}
        docPath={refs.docPath}
        siteUrl={refs.siteUrl}
        artifactDirRel={ARTIFACT_DIR_REL}
        noTevmRule={NO_TEVM_RULE()}
        noSmithersRule={NO_SMITHERS_RULE}
        userSteering={USER_STEERING}
      />
    </Panel>
  );
}
