/**
 * BEP Export Utilities
 *
 * Generates well-structured markdown files from BEP data for local use with agents.
 * The export creates an organized folder structure with all proposal content,
 * versions, comments, issues, and decisions.
 */

// Types for the export data
export interface ExportBep {
  _id: string;
  number: number;
  title: string;
  status: string;
  shepherdNames: string[];
  content: string;
  createdAt: number;
  updatedAt: number;
}

export interface ExportPage {
  _id: string;
  slug: string;
  title: string;
  content: string;
  order: number;
}

export interface ExportComment {
  _id: string;
  type: string;
  content: string;
  authorName: string;
  parentId?: string;
  pageId?: string;
  versionId?: string;
  versionNumber?: number; // The version number this comment belongs to
  resolved: boolean;
  resolvedByName?: string;
  resolvedAt?: number;
  createdAt: number;
  reactions?: {
    thumbsUp?: string[];
    thumbsDown?: string[];
    heart?: string[];
    thinking?: string[];
  };
  // Block comment anchor data (Tiptap node-based)
  anchor?: {
    nodeId: string;
    nodeType: string;
    nodeText: string;
  };
}

export interface ExportDecision {
  _id: string;
  title: string;
  description: string;
  rationale?: string;
  participantNames: string[];
  decidedAt: number;
}

export interface ExportIssue {
  _id: string;
  title: string;
  description?: string;
  raisedByName: string;
  assignedToName?: string;
  resolved: boolean;
  resolution?: string;
  resolvedAt?: number;
  createdAt: number;
}

export interface ExportVersion {
  _id: string;
  version: number;
  title: string;
  content: string;
  pagesSnapshot?: Array<{
    slug: string;
    title: string;
    content: string;
    order: number;
  }>;
  editorName: string;
  editNote?: string;
  createdAt: number;
}

export interface ExportSummary {
  _id: string;
  content: string;
  status: string;
  reviewerName?: string;
  periodStart: number;
  periodEnd: number;
  createdAt: number;
  themes?: Array<{
    name: string;
    summary: string;
    sentiment: string;
  }>;
}

export interface ExportData {
  bep: ExportBep;
  pages: ExportPage[];
  comments: ExportComment[];
  decisions: ExportDecision[];
  issues: ExportIssue[];
  versions: ExportVersion[];
  summaries: ExportSummary[];
  currentVersion: number;
  exportedAt: number;
}

// ─────────────────────────────────────────────────────────────────────────────
// Date formatting utilities
// ─────────────────────────────────────────────────────────────────────────────

function formatDate(timestamp: number): string {
  return new Date(timestamp).toISOString().split("T")[0];
}

function formatDateTime(timestamp: number): string {
  return new Date(timestamp).toISOString().replace("T", " ").slice(0, 19);
}

// ─────────────────────────────────────────────────────────────────────────────
// BEP number formatting
// ─────────────────────────────────────────────────────────────────────────────

function formatBepNumber(num: number): string {
  return `BEP-${String(num).padStart(3, "0")}`;
}

// ─────────────────────────────────────────────────────────────────────────────
// Frontmatter Generation
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Generate YAML frontmatter for a markdown file.
 */
function generateFrontmatter(fields: Record<string, string | number | string[]>): string {
  const lines = ["---"];
  for (const [key, value] of Object.entries(fields)) {
    if (Array.isArray(value)) {
      if (value.length > 0) {
        lines.push(`${key}:`);
        for (const item of value) {
          lines.push(`  - ${item}`);
        }
      }
    } else {
      lines.push(`${key}: ${typeof value === "string" && value.includes(":") ? `"${value}"` : value}`);
    }
  }
  lines.push("---", "");
  return lines.join("\n");
}

// ─────────────────────────────────────────────────────────────────────────────
// Block Comment Embedding Utilities
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Format a single block comment with minimal metadata.
 */
function formatBlockComment(comment: ExportComment): string {
  const context = (comment.anchor?.nodeText ?? "").slice(0, 60);
  const contextLine = context ? ` | "${context}${context.length >= 60 ? "..." : ""}"` : "";

  return `<!-- @${comment.type} by ${comment.authorName}${contextLine} -->
> ${comment.content.split("\n").join("\n> ")}
<!-- /@${comment.type} -->`;
}

/**
 * Format general (non-block) comments section for the bottom of a content file.
 */
function formatGeneralCommentsSection(comments: ExportComment[]): string {
  if (comments.length === 0) return "";

  // Build comment tree
  const rootComments = comments.filter((c) => !c.parentId);
  const repliesByParent = new Map<string, ExportComment[]>();

  for (const comment of comments) {
    if (comment.parentId) {
      const existing = repliesByParent.get(comment.parentId) || [];
      existing.push(comment);
      repliesByParent.set(comment.parentId, existing);
    }
  }

  // Sort root comments by date
  rootComments.sort((a, b) => a.createdAt - b.createdAt);

  const lines: string[] = ["", "---", "", "<!-- comments -->", ""];

  for (const comment of rootComments) {
    lines.push(`<!-- @${comment.type} by ${comment.authorName} -->`);
    lines.push(`> ${comment.content.split("\n").join("\n> ")}`);

    // Add replies inline
    const replies = repliesByParent.get(comment._id) || [];
    replies.sort((a, b) => a.createdAt - b.createdAt);
    for (const reply of replies) {
      lines.push(`>> ${reply.authorName}: ${reply.content.split("\n").join("\n>> ")}`);
    }

    lines.push(`<!-- /@${comment.type} -->`, "");
  }

  lines.push("<!-- /comments -->");

  return lines.join("\n");
}

/**
 * Embed comments into content markdown.
 * Block comments are listed in a section at the bottom with their context.
 * Returns the content with embedded comments.
 *
 * @param content - The markdown content to embed comments in
 * @param comments - All comments to consider
 * @param currentVersion - The current version number
 * @param pageId - Optional page ID to filter comments for a specific page
 * @param includeOutdated - If false (default), only include comments from the current version
 * @param targetVersion - If provided, only include comments from this specific version (for history)
 */
export function embedCommentsInContent(
  content: string,
  comments: ExportComment[],
  currentVersion: number,
  pageId?: string,
  includeOutdated: boolean = false,
  targetVersion?: number
): string {
  // Filter comments for this content (main or specific page)
  let relevantComments = comments.filter((c) => {
    if (pageId) {
      return c.pageId === pageId;
    }
    return !c.pageId; // Main content has no pageId
  });

  // Filter by version if needed
  if (targetVersion !== undefined) {
    // For history: only include comments from this specific version
    relevantComments = relevantComments.filter((c) => c.versionNumber === targetVersion);
  } else if (!includeOutdated) {
    // For current content: only include comments from the current version
    relevantComments = relevantComments.filter(
      (c) => c.versionNumber === undefined || c.versionNumber === currentVersion
    );
  }

  // Separate block and general comments (include replies for general comments)
  const blockComments = relevantComments.filter((c) => c.anchor && !c.parentId);
  const generalComments = relevantComments.filter((c) => !c.anchor); // includes replies
  const rootGeneralComments = generalComments.filter((c) => !c.parentId);

  if (blockComments.length === 0 && rootGeneralComments.length === 0) {
    return content;
  }

  const result: string[] = [content];

  // Add block comments section at the bottom
  if (blockComments.length > 0) {
    blockComments.sort((a, b) => a.createdAt - b.createdAt);

    result.push("", "---", "", "<!-- block-comments -->", "");

    for (const comment of blockComments) {
      result.push(formatBlockComment(comment));
      result.push("");
    }

    result.push("<!-- /block-comments -->");
  }

  // Add general comments section at the bottom (pass all including replies)
  if (rootGeneralComments.length > 0) {
    result.push(formatGeneralCommentsSection(generalComments));
  }

  return result.join("\n");
}

// ─────────────────────────────────────────────────────────────────────────────
// Main README.md generation
// ─────────────────────────────────────────────────────────────────────────────

export function generateReadme(data: ExportData): string {
  const { bep, pages, comments, currentVersion } = data;
  const bepNum = formatBepNumber(bep.number);

  // Generate frontmatter
  const frontmatter = generateFrontmatter({
    title: `${bepNum}: ${bep.title}`,
    status: bep.status,
    version: currentVersion,
    created: formatDate(bep.createdAt),
    updated: formatDate(bep.updatedAt),
    shepherds: bep.shepherdNames,
  });

  let md = frontmatter;
  md += `# ${bepNum}: ${bep.title}\n\n`;

  // Main content with embedded comments
  if (bep.content) {
    const contentWithComments = embedCommentsInContent(
      bep.content,
      comments,
      currentVersion,
      undefined // main content has no pageId
    );
    md += `${contentWithComments}\n\n`;
  }

  // Additional pages (linked)
  if (pages.length > 0) {
    md += `---\n\n## Additional Pages\n\n`;
    for (const page of pages) {
      md += `- [${page.title}](pages/${page.slug}.md)\n`;
    }
    md += `\n`;
  }

  return md.trim() + "\n";
}

// ─────────────────────────────────────────────────────────────────────────────
// Issues markdown generation
// ─────────────────────────────────────────────────────────────────────────────

export function generateIssuesMd(data: ExportData): string {
  const { issues, bep } = data;

  const openIssues = issues.filter((i) => !i.resolved);
  const resolvedIssues = issues.filter((i) => i.resolved);

  let md = `# Issues - ${formatBepNumber(bep.number)}

> Open issues: ${openIssues.length}
> Resolved issues: ${resolvedIssues.length}

---

## Open Issues

`;

  if (openIssues.length === 0) {
    md += `*No open issues*

`;
  } else {
    for (const issue of openIssues) {
      md += formatIssue(issue);
    }
  }

  md += `## Resolved Issues

`;

  if (resolvedIssues.length === 0) {
    md += `*No resolved issues*

`;
  } else {
    for (const issue of resolvedIssues) {
      md += formatIssue(issue);
    }
  }

  return md.trim() + "\n";
}

function formatIssue(issue: ExportIssue): string {
  const status = issue.resolved ? "✅ RESOLVED" : "⚠️ OPEN";

  let md = `### ${issue.title}

| Field | Value |
|-------|-------|
| **Status** | ${status} |
| **Raised by** | ${issue.raisedByName} |
| **Raised on** | ${formatDateTime(issue.createdAt)} |
`;

  if (issue.assignedToName) {
    md += `| **Assigned to** | ${issue.assignedToName} |
`;
  }

  if (issue.resolved && issue.resolvedAt) {
    md += `| **Resolved on** | ${formatDateTime(issue.resolvedAt)} |
`;
  }

  md += `
`;

  if (issue.description) {
    md += `**Description:**

${issue.description}

`;
  }

  if (issue.resolution) {
    md += `**Resolution:**

${issue.resolution}

`;
  }

  md += `---

`;

  return md;
}

// ─────────────────────────────────────────────────────────────────────────────
// Decisions markdown generation
// ─────────────────────────────────────────────────────────────────────────────

export function generateDecisionsMd(data: ExportData): string {
  const { decisions, bep } = data;

  if (decisions.length === 0) {
    return `# Decisions - ${formatBepNumber(bep.number)}

No decisions recorded yet.
`;
  }

  // Sort by decidedAt descending (most recent first)
  const sortedDecisions = [...decisions].sort(
    (a, b) => b.decidedAt - a.decidedAt
  );

  let md = `# Decisions - ${formatBepNumber(bep.number)}

> Total decisions: ${decisions.length}

This document records all decisions made during the proposal review process.

---

`;

  for (const decision of sortedDecisions) {
    md += `## ${decision.title}

| Field | Value |
|-------|-------|
| **Decided on** | ${formatDateTime(decision.decidedAt)} |
| **Participants** | ${decision.participantNames.join(", ")} |

**Decision:**

${decision.description}

`;

    if (decision.rationale) {
      md += `**Rationale:**

${decision.rationale}

`;
    }

    md += `---

`;
  }

  return md.trim() + "\n";
}

// ─────────────────────────────────────────────────────────────────────────────
// Version history generation
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Generate a version index file listing all versions with metadata.
 */
export function generateVersionIndex(data: ExportData): string {
  const { versions, bep, comments, currentVersion } = data;

  if (versions.length === 0) {
    return `# Version History - ${formatBepNumber(bep.number)}

No version history available.
`;
  }

  let md = `# Version History - ${formatBepNumber(bep.number)}

> Total versions: ${versions.length}
> Current version: v${currentVersion}

Each version folder contains the content snapshot and comments from that version.

---

`;

  // Versions are already sorted descending
  for (const version of versions) {
    const versionComments = comments.filter(
      (c) => c.versionNumber === version.version && !c.parentId
    );
    const isCurrent = version.version === currentVersion;

    md += `## Version ${version.version}${isCurrent ? " (Current)" : ""}

| Field | Value |
|-------|-------|
| **Date** | ${formatDateTime(version.createdAt)} |
| **Editor** | ${version.editorName} |
| **Title** | ${version.title} |
| **Comments** | ${versionComments.length} |
`;

    if (version.editNote) {
      md += `| **Note** | ${version.editNote} |
`;
    }

    md += `
**Files:** [v${version.version}_readme.md](v${version.version}/v${version.version}_readme.md)`;

    if (version.pagesSnapshot && version.pagesSnapshot.length > 0) {
      for (const page of version.pagesSnapshot) {
        md += `, [v${version.version}_${page.slug}.md](v${version.version}/v${version.version}_${page.slug}.md)`;
      }
    }

    md += `

---

`;
  }

  return md.trim() + "\n";
}

/**
 * Generate content file for a specific version with its comments.
 */
export function generateVersionContent(
  version: ExportVersion,
  comments: ExportComment[],
  currentVersion: number,
  bep: ExportBep
): string {
  const bepNum = formatBepNumber(bep.number);

  let md = `# ${bepNum}: ${version.title} (v${version.version})

| Field | Value |
|-------|-------|
| **Version** | ${version.version} |
| **Date** | ${formatDateTime(version.createdAt)} |
| **Editor** | ${version.editorName} |
`;

  if (version.editNote) {
    md += `| **Note** | ${version.editNote} |
`;
  }

  md += `
---

`;

  // Embed comments from this specific version
  if (version.content) {
    const contentWithComments = embedCommentsInContent(
      version.content,
      comments,
      currentVersion,
      undefined, // main content
      false, // don't include outdated (we're filtering by targetVersion)
      version.version // only comments from this version
    );
    md += `${contentWithComments}
`;
  }

  return md.trim() + "\n";
}

/**
 * Generate page content file for a specific version.
 * Note: Page comments can't be reliably matched to historical page snapshots
 * since page snapshots don't preserve the original pageId.
 */
export function generateVersionPageContent(
  version: ExportVersion,
  page: { slug: string; title: string; content: string }
): string {
  const md = `# ${page.title} (v${version.version})

---

${page.content}
`;

  return md.trim() + "\n";
}

/**
 * Generate all files for a specific version's history folder.
 */
export function generateVersionFiles(
  version: ExportVersion,
  comments: ExportComment[],
  currentVersion: number,
  bep: ExportBep
): ExportFile[] {
  const files: ExportFile[] = [];
  const vPrefix = `v${version.version}`;

  // Main content file for this version
  files.push({
    path: `history/${vPrefix}/${vPrefix}_readme.md`,
    content: generateVersionContent(version, comments, currentVersion, bep),
  });

  // Page content files for this version
  if (version.pagesSnapshot && version.pagesSnapshot.length > 0) {
    for (const page of version.pagesSnapshot) {
      files.push({
        path: `history/${vPrefix}/${vPrefix}_${page.slug}.md`,
        content: generateVersionPageContent(version, page),
      });
    }
  }

  return files;
}

// ─────────────────────────────────────────────────────────────────────────────
// AI Summaries markdown generation
// ─────────────────────────────────────────────────────────────────────────────

export function generateSummariesMd(data: ExportData): string {
  const { summaries, bep } = data;

  if (summaries.length === 0) {
    return `# AI Summaries - ${formatBepNumber(bep.number)}

No AI summaries have been generated yet.
`;
  }

  const sortedSummaries = [...summaries].sort(
    (a, b) => b.createdAt - a.createdAt
  );

  let md = `# AI Summaries - ${formatBepNumber(bep.number)}

> Total summaries: ${summaries.length}

These are AI-generated summaries of the discussion feedback.

---

`;

  for (const summary of sortedSummaries) {
    const statusBadge =
      summary.status === "approved"
        ? "✅ Approved"
        : summary.status === "applied"
          ? "📝 Applied"
          : "📋 Draft";

    md += `## Summary - ${formatDateTime(summary.createdAt)}

| Field | Value |
|-------|-------|
| **Status** | ${statusBadge} |
| **Period** | ${formatDate(summary.periodStart)} to ${formatDate(summary.periodEnd)} |
`;

    if (summary.reviewerName) {
      md += `| **Reviewed by** | ${summary.reviewerName} |
`;
    }

    md += `
${summary.content}

`;

    if (summary.themes && summary.themes.length > 0) {
      md += `### Themes Identified

`;
      for (const theme of summary.themes) {
        const sentimentEmoji =
          theme.sentiment === "positive"
            ? "🟢"
            : theme.sentiment === "concern"
              ? "🔴"
              : "🟡";
        md += `- **${theme.name}** ${sentimentEmoji}: ${theme.summary}
`;
      }
      md += `
`;
    }

    md += `---

`;
  }

  return md.trim() + "\n";
}

// ─────────────────────────────────────────────────────────────────────────────
// Metadata JSON generation (for agent consumption)
// ─────────────────────────────────────────────────────────────────────────────

export function generateMetadataJson(data: ExportData): string {
  const { bep, pages, comments, decisions, issues, versions, summaries, currentVersion, exportedAt } = data;

  // Count outdated comments
  const outdatedComments = comments.filter(
    (c) => c.versionNumber !== undefined && c.versionNumber < currentVersion && !c.parentId
  );

  // Count comments by version for history
  const commentsByVersion: Record<number, number> = {};
  for (const version of versions) {
    const versionComments = comments.filter(
      (c) => c.versionNumber === version.version && !c.parentId
    );
    commentsByVersion[version.version] = versionComments.length;
  }

  // Current version comments only (not outdated)
  const currentVersionComments = comments.filter(
    (c) => c.versionNumber === currentVersion && !c.parentId
  );
  const currentBlockComments = currentVersionComments.filter((c) => c.anchor);
  const currentGeneralComments = currentVersionComments.filter((c) => !c.anchor);

  const metadata = {
    exportInfo: {
      exportedAt: new Date(exportedAt).toISOString(),
      format: "bep-export-v4", // Updated format version for per-version history folders
    },
    bep: {
      id: bep._id,
      number: bep.number,
      title: bep.title,
      status: bep.status,
      shepherds: bep.shepherdNames,
      currentVersion: currentVersion,
      createdAt: new Date(bep.createdAt).toISOString(),
      updatedAt: new Date(bep.updatedAt).toISOString(),
    },
    stats: {
      pageCount: pages.length,
      totalCommentCount: comments.filter((c) => !c.parentId).length,
      currentVersionCommentCount: currentVersionComments.length,
      outdatedCommentCount: outdatedComments.length,
      decisionCount: decisions.length,
      issueCount: issues.length,
      openIssueCount: issues.filter((i) => !i.resolved).length,
      versionCount: versions.length,
      summaryCount: summaries.length,
    },
    comments: {
      // Current version comments (shown in README and pages)
      current: {
        inline: currentBlockComments.length,
        general: currentGeneralComments.length,
      },
      // Comments per version (in history folders)
      byVersion: commentsByVersion,
    },
    files: [
      "README.md",
      "AGENT_CONTEXT.md",
      ...pages.map((p) => `pages/${p.slug}.md`),
      "discussion/issues.md",
      "discussion/decisions.md",
      "history/versions.md",
      ...versions.flatMap((v) => {
        const vPrefix = `v${v.version}`;
        const versionFiles = [`history/${vPrefix}/${vPrefix}_readme.md`];
        if (v.pagesSnapshot) {
          for (const page of v.pagesSnapshot) {
            versionFiles.push(`history/${vPrefix}/${vPrefix}_${page.slug}.md`);
          }
        }
        return versionFiles;
      }),
      ...(summaries.length > 0 ? ["history/summaries.md"] : []),
      "metadata.json",
    ],
  };

  return JSON.stringify(metadata, null, 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// Agent context file generation
// ─────────────────────────────────────────────────────────────────────────────

export function generateAgentContext(data: ExportData): string {
  const { bep, pages, comments, decisions, issues, currentVersion, summaries, versions } = data;
  const bepNum = formatBepNumber(bep.number);

  const openIssues = issues.filter((i) => !i.resolved);

  // Current version comments only (shown in main content)
  const currentVersionComments = comments.filter(
    (c) => c.versionNumber === currentVersion && !c.parentId
  );
  const currentBlockComments = currentVersionComments.filter((c) => c.anchor);
  const currentGeneralComments = currentVersionComments.filter((c) => !c.anchor);
  const unresolvedCurrentComments = currentVersionComments.filter(
    (c) => !c.resolved && c.type === "concern"
  );

  // Outdated comments (in history folders)
  const outdatedComments = comments.filter(
    (c) => c.versionNumber !== undefined && c.versionNumber < currentVersion && !c.parentId
  );

  // Extract first paragraph as summary if content exists
  const contentSummary = bep.content
    ? bep.content.split("\n\n")[0].slice(0, 500)
    : "No content available.";

  const md = `# Agent Context - ${bepNum}

This file provides context for AI agents working with this BEP.

## Quick Summary

- **Title:** ${bep.title}
- **Status:** ${bep.status}
- **Shepherds:** ${bep.shepherdNames.join(", ") || "None"}
- **Current Version:** v${currentVersion}

## Current State

${contentSummary}${contentSummary.length >= 500 ? "..." : ""}

## Pages

- Main Content (README.md) - includes current version comments only
${pages.length > 0 ? pages.map((p) => `- ${p.title} (pages/${p.slug}.md)`).join("\n") : ""}

## Comment Overview

**README and pages contain only current version (v${currentVersion}) comments.**
Older version comments are preserved in the \`history/\` folder.

- **Current version comments:** ${currentVersionComments.length} (${currentBlockComments.length} inline, ${currentGeneralComments.length} general)
- **Unresolved concerns:** ${unresolvedCurrentComments.length}
- **Historical comments:** ${outdatedComments.length} (in history/v1/, history/v2/, etc.)

## Outstanding Items

### Open Issues (${openIssues.length})

${
  openIssues.length > 0
    ? openIssues.map((i) => `- **${i.title}**: ${i.description || "No description"}`).join("\n")
    : "*No open issues*"
}

### Unresolved Concerns (${unresolvedCurrentComments.length})

${
  unresolvedCurrentComments.length > 0
    ? unresolvedCurrentComments
        .slice(0, 5)
        .map((c) => `- ${c.authorName}: ${c.content.slice(0, 100)}${c.content.length > 100 ? "..." : ""}`)
        .join("\n")
    : "*No unresolved concerns in current version*"
}

## Recent Decisions (${Math.min(decisions.length, 3)})

${
  decisions.length > 0
    ? decisions
        .slice(0, 3)
        .map((d) => `- **${d.title}**: ${d.description.slice(0, 100)}${d.description.length > 100 ? "..." : ""}`)
        .join("\n")
    : "*No decisions recorded*"
}

## File Structure

\`\`\`
${bepNum}/
├── README.md               # Current content + v${currentVersion} comments
├── AGENT_CONTEXT.md        # This file
├── metadata.json           # Machine-readable metadata${pages.length > 0 ? `
├── pages/                  # Additional pages + v${currentVersion} comments
${pages.map((p) => `│   └── ${p.slug}.md`).join("\n")}` : ""}
├── discussion/
│   ├── issues.md           # Open and resolved issues
│   └── decisions.md        # Recorded decisions
└── history/
    ├── versions.md         # Version index${versions.map((v) => `
    ├── v${v.version}/              # Version ${v.version} content + comments
    │   └── v${v.version}_readme.md`).join("")}${summaries.length > 0 ? `
    └── summaries.md        # AI-generated summaries` : ""}
\`\`\`

## How to Use

1. Start with \`README.md\` for the current proposal content with current version feedback
2. Check \`pages/\` folder for additional documentation
3. Look for \`<!-- block-comments -->\` section for block-specific feedback
4. Look for \`<!-- comments -->\` section for general discussion
5. Check \`discussion/issues.md\` for outstanding issues to address
6. Reference \`discussion/decisions.md\` for established decisions

### Comment Format

Comments use this minimal format:
\`\`\`markdown
<!-- @discussion by AuthorName | "context text..." -->
> The comment content
<!-- /@discussion -->
\`\`\`
`;

  return md.trim() + "\n";
}

// ─────────────────────────────────────────────────────────────────────────────
// Generate all files for ZIP export
// ─────────────────────────────────────────────────────────────────────────────

export interface ExportFile {
  path: string;
  content: string;
}

export function generateAllExportFiles(data: ExportData): ExportFile[] {
  const { bep, comments, currentVersion, pages, versions } = data;

  const files: ExportFile[] = [
    // Main content with inline comments embedded (current version comments only)
    { path: "README.md", content: generateReadme(data) },
    // AI-friendly summary
    { path: "AGENT_CONTEXT.md", content: generateAgentContext(data) },
    // Machine-readable metadata
    { path: "metadata.json", content: generateMetadataJson(data) },
    // Discussion files (comments are now embedded, but issues/decisions remain separate)
    { path: "discussion/issues.md", content: generateIssuesMd(data) },
    { path: "discussion/decisions.md", content: generateDecisionsMd(data) },
    // History - version index
    { path: "history/versions.md", content: generateVersionIndex(data) },
  ];

  // Add page files with embedded comments (current version comments only)
  for (const page of pages) {
    const contentWithComments = embedCommentsInContent(
      page.content,
      comments,
      currentVersion,
      page._id,
      false // don't include outdated comments
    );
    const pageFrontmatter = generateFrontmatter({
      title: page.title,
      slug: page.slug,
      order: page.order,
    });
    files.push({
      path: `pages/${page.slug}.md`,
      content: `${pageFrontmatter}# ${page.title}\n\n${contentWithComments}\n`,
    });
  }

  // Add per-version history files with their respective comments
  for (const version of versions) {
    const versionFiles = generateVersionFiles(version, comments, currentVersion, bep);
    files.push(...versionFiles);
  }

  // Only include summaries if there are any
  if (data.summaries.length > 0) {
    files.push({
      path: "history/summaries.md",
      content: generateSummariesMd(data),
    });
  }

  return files;
}
