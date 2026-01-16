/**
 * AI Prompt Templates for BEP Analysis
 *
 * These prompts are designed to help Claude analyze BEP discussions
 * and provide helpful answers to user questions.
 */

export interface BepContext {
  number: number;
  title: string;
  status: string;
  content: string;
}

export interface CommentContext {
  id: string;
  type: string;
  content: string;
  authorName: string;
  createdAt: number;
  resolved: boolean;
  parentId?: string;
}

export interface VersionContext {
  version: number;
  title: string;
  content: string;
  createdAt: number;
  editedBy: string;
  editNote?: string;
}

export interface IssueContext {
  title: string;
  description?: string;
  raisedBy: string;
  resolved: boolean;
  resolution?: string;
}

export interface DecisionContext {
  title: string;
  description: string;
  rationale?: string;
  participants: string[];
  decidedAt: number;
}

export interface AIAssistantContext {
  bep: BepContext;
  fromVersion?: VersionContext;
  toVersion?: VersionContext;
  fromVersionComments: CommentContext[];
  toVersionComments: CommentContext[];
  issues: IssueContext[];
  decisions: DecisionContext[];
}

export type QuickAction = "summarize_changes" | "list_addressed_concerns" | "custom";

/**
 * Builds the AI assistant prompt for interactive Q&A with optional version comparison
 */
export function buildAIAssistantPrompt(
  context: AIAssistantContext,
  userQuestion: string,
  quickAction?: QuickAction
): string {
  const bepNumber = `BEP-${String(context.bep.number).padStart(3, "0")}`;

  // Format version content
  const formatVersion = (v: VersionContext) => {
    const date = new Date(v.createdAt).toISOString().split("T")[0];
    return `**Version ${v.version}** (${date}, edited by ${v.editedBy})
${v.editNote ? `Edit note: ${v.editNote}` : ""}

${v.content || "(No content)"}`;
  };

  // Format comments
  const formatComments = (comments: CommentContext[], label: string) => {
    if (comments.length === 0) return `### ${label}\n\nNo comments.`;

    const formatted = comments
      .map((c) => {
        const date = new Date(c.createdAt).toISOString().split("T")[0];
        const resolved = c.resolved ? " [RESOLVED]" : "";
        const reply = c.parentId ? " (reply)" : "";
        return `[${c.type.toUpperCase()}${resolved}${reply}] ${c.authorName} (${date}):\n${c.content}`;
      })
      .join("\n\n---\n\n");

    return `### ${label} (${comments.length} comments)\n\n${formatted}`;
  };

  // Format issues
  const formatIssues = () => {
    if (context.issues.length === 0) return "### Open Issues\n\nNo issues tracked.";

    const formatted = context.issues
      .map((i) => {
        const status = i.resolved ? "✓ RESOLVED" : "○ OPEN";
        return `[${status}] **${i.title}**
Raised by: ${i.raisedBy}
${i.description || ""}
${i.resolved && i.resolution ? `Resolution: ${i.resolution}` : ""}`;
      })
      .join("\n\n");

    return `### Issues (${context.issues.filter((i) => !i.resolved).length} open, ${context.issues.filter((i) => i.resolved).length} resolved)\n\n${formatted}`;
  };

  // Format decisions
  const formatDecisions = () => {
    if (context.decisions.length === 0) return "### Decisions\n\nNo decisions recorded.";

    const formatted = context.decisions
      .map((d) => {
        const date = new Date(d.decidedAt).toISOString().split("T")[0];
        return `**${d.title}** (${date})
Participants: ${d.participants.join(", ")}
${d.description}
${d.rationale ? `Rationale: ${d.rationale}` : ""}`;
      })
      .join("\n\n");

    return `### Decisions (${context.decisions.length} recorded)\n\n${formatted}`;
  };

  // Build the question based on quick action or custom question
  let effectiveQuestion = userQuestion;
  let taskPrefix = "";

  const hasVersionComparison = context.fromVersion && context.toVersion;

  if (quickAction === "summarize_changes" && hasVersionComparison) {
    taskPrefix = `You are helping analyze changes between versions of a BEP proposal. Your task is to summarize what changed and why.`;
    effectiveQuestion = effectiveQuestion || "Summarize the changes between these versions. What was modified, added, or removed? How do the comments and feedback relate to these changes?";
  } else if (quickAction === "list_addressed_concerns" && hasVersionComparison) {
    taskPrefix = `You are helping track how community feedback has been addressed in a BEP proposal.`;
    effectiveQuestion = effectiveQuestion || "List the concerns that were raised in the earlier version and explain how they have been addressed (or not addressed) in the later version.";
  } else {
    taskPrefix = `You are an AI assistant helping analyze and understand a BAML Enhancement Proposal (BEP) and its discussion history.`;
  }

  // Build context sections based on what's available
  let versionSection = "";
  let commentsSection = "";

  if (hasVersionComparison) {
    // Version comparison mode
    versionSection = `## Version Comparison

### From Version
${formatVersion(context.fromVersion!)}

### To Version
${formatVersion(context.toVersion!)}`;

    commentsSection = `## Comments by Version

${formatComments(context.fromVersionComments, `Version ${context.fromVersion!.version} Comments`)}

${formatComments(context.toVersionComments, `Version ${context.toVersion!.version} Comments`)}`;
  } else if (context.toVersion) {
    // Single version mode
    versionSection = `## Current Version
${formatVersion(context.toVersion)}`;

    commentsSection = `## Comments

${formatComments(context.toVersionComments, "Comments")}`;
  } else {
    // No version context - just use BEP content
    versionSection = `## Proposal Content

${context.bep.content || "(No content)"}`;

    commentsSection = `## Comments

${formatComments(context.toVersionComments, "Comments")}`;
  }

  return `${taskPrefix}

## Proposal Overview

**${bepNumber}: ${context.bep.title}**
**Status:** ${context.bep.status}

${versionSection}

${commentsSection}

${formatIssues()}

${formatDecisions()}

---

## Your Task

${effectiveQuestion}

Please provide a clear, structured response that:
- Directly answers the question asked
- References specific content from the versions and comments when relevant
- Highlights any actionable insights or recommendations

Be concise but thorough. Use markdown formatting for readability.`;
}
