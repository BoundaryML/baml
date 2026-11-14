// The coding agents a comment can say it was written by, on behalf of the member whose token
// posted it: a name to show and the GitHub account whose picture stands for it (Claude's is the
// one GitHub shows for Claude as a co-author). Any other agent shows its name with a generic icon.
// No functions here: the site imports it too.

export const KNOWN_AGENTS: Record<string, { name: string; github: string }> = {
  claude: { name: "Claude", github: "claude" },
  codex: { name: "Codex", github: "codex" },
  gemini: { name: "Gemini", github: "gemini-cli" },
  cursor: { name: "Cursor", github: "cursor" },
  copilot: { name: "Copilot", github: "github" },
};

/** An agent's id as stored: lowercase letters, digits and hyphens, at most 32. */
export const AGENT_ID = /^[a-z0-9][a-z0-9-]{0,31}$/;

/** How an agent is named on a comment: its known name, or its id as given. */
export function agentName(id: string): string {
  return KNOWN_AGENTS[id]?.name ?? id;
}
