// Real locally configured agent adapters shared by the task modules.
// Discovery + clear failure for absent CLIs happens in the `preflight` compute
// task before any agent task can run. Bypass flags are required for detached
// runs. Workflow scaffolding; removed by the final cleanup task.
import { ClaudeCodeAgent, CodexAgent, OpenCodeAgent } from "smthrs";

export const codexSol = new CodexAgent({
  model: "gpt-5.6-sol",
  config: { model_reasoning_effort: "xhigh" },
  sandbox: "danger-full-access",
  dangerouslyBypassApprovalsAndSandbox: true,
  skipGitRepoCheck: true,
});
// Same Codex Sol seat at high (not xhigh) effort for the very long build/fix tasks.
export const codexSolBuilder = new CodexAgent({
  model: "gpt-5.6-sol",
  config: { model_reasoning_effort: "high" },
  sandbox: "danger-full-access",
  dangerouslyBypassApprovalsAndSandbox: true,
  skipGitRepoCheck: true,
});
export const claudeFable = new ClaudeCodeAgent({
  model: "claude-fable-5",
  permissionMode: "bypassPermissions",
  dangerouslySkipPermissions: true,
});
// Kimi K3 through OpenCode (OpenCode's kimi-for-coding provider).
export const kimiOpenCode = new OpenCodeAgent({
  model: "kimi-for-coding/k3",
  yolo: true,
});
// Cost-tier routing for commit-plan lanes (user requirement): Sol for difficult or
// important commits, Terra for straightforward ones, Luna for trivial black/white
// ones. The synthesized commit plan assigns a tier to every commit.
export const codexTerra = new CodexAgent({
  model: "gpt-5.6-terra",
  sandbox: "danger-full-access",
  dangerouslyBypassApprovalsAndSandbox: true,
  skipGitRepoCheck: true,
});
export const codexLuna = new CodexAgent({
  model: "gpt-5.6-luna",
  sandbox: "danger-full-access",
  dangerouslyBypassApprovalsAndSandbox: true,
  skipGitRepoCheck: true,
});
export const TIER_AGENTS = { sol: codexSolBuilder, terra: codexTerra, luna: codexLuna } as const;
export type Tier = keyof typeof TIER_AGENTS;
// Fourth seat of the final sign-off panel (user requirement): Claude Opus.
export const claudeOpus = new ClaudeCodeAgent({
  model: "claude-opus-5",
  permissionMode: "bypassPermissions",
  dangerouslySkipPermissions: true,
});
// Failover chain for the Fable-fronted gating tasks (synthesize-plan, review,
// finalize-doc, panel moderator): the first execution lost ~8h to a dead engine
// sitting in a stale Fable quota park; with an AgentLike[] chain a parked Fable
// fails over to Opus instead of stalling the run.
export const fableWithOpusFailover = [claudeFable, claudeOpus];
