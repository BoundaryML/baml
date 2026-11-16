// The coding agents a comment can say it was written by, on behalf of the member whose token
// posted it: a name to show and the GitHub account whose picture stands for it (Claude's is the
// one GitHub shows for Claude as a co-author). Any other agent shows its name with a generic icon.
// No functions here: the site imports it too.

export const KNOWN_AGENTS: Record<string, { name: string; github: string; what: string; aliases: string[] }> = {
  claude: {
    name: "Claude",
    github: "claude",
    what: "Claude Code, or Claude anywhere",
    aliases: ["claude-code", "claudecode", "anthropic"],
  },
  codex: {
    name: "Codex",
    github: "codex",
    what: "OpenAI Codex, or ChatGPT",
    aliases: ["openai", "openai-codex", "codex-cli", "chatgpt", "gpt"],
  },
  gemini: {
    name: "Gemini",
    github: "gemini-cli",
    what: "Gemini CLI, or Gemini anywhere",
    aliases: ["gemini-cli", "google", "google-gemini"],
  },
  cursor: { name: "Cursor", github: "cursor", what: "Cursor's agent", aliases: ["cursor-agent"] },
  copilot: {
    name: "Copilot",
    github: "github",
    what: "GitHub Copilot",
    aliases: ["github-copilot", "gh-copilot"],
  },
};

/** The known agents' ids, for help and error text: "claude, codex, gemini, cursor, copilot". */
export const KNOWN_AGENT_IDS = Object.keys(KNOWN_AGENTS).join(", ");

/**
 * The id a given agent name is stored as: lowercased, and a known agent's other names (chatgpt,
 * claude-code, gemini-cli, ...) mapped to its id, so an agent that names itself loosely still gets
 * its picture. Anything else is kept as given: a team's own agent.
 */
export function normalizeAgent(given: string): string {
  const id = given.trim().toLowerCase().replace(/[\s_]+/g, "-");
  const known = Object.entries(KNOWN_AGENTS).find(([, agent]) => agent.aliases.includes(id));
  return known ? known[0] : id;
}

/** An agent's id as stored: lowercase letters, digits and hyphens, at most 32. */
export const AGENT_ID = /^[a-z0-9][a-z0-9-]{0,31}$/;

/** How an agent is named on a comment: its known name, or its id as given. */
export function agentName(id: string): string {
  return KNOWN_AGENTS[id]?.name ?? id;
}
