/**
 * BEP Export All Utilities
 *
 * Generates a lightweight reference bundle of all BEPs for AI context.
 * Status is prominently featured to give more weight to mature proposals.
 */

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

export interface ExportAllBep {
  _id: string;
  number: number;
  title: string;
  status: string;
  content: string;
  shepherdNames: string[];
  currentVersion: number;
  openIssueCount: number;
  createdAt: number;
  updatedAt: number;
  pages: ExportAllPage[];
}

export interface ExportAllPage {
  _id: string;
  slug: string;
  title: string;
  content: string;
  order: number;
}

export interface ExportAllData {
  beps: ExportAllBep[];
  exportedAt: number;
}

export interface ExportAllFile {
  path: string;
  content: string;
}

// ─────────────────────────────────────────────────────────────────────────────
// Status Utilities
// ─────────────────────────────────────────────────────────────────────────────

const STATUS_ORDER: Record<string, number> = {
  implemented: 1,
  accepted: 2,
  proposed: 3,
  draft: 4,
  superseded: 5,
  rejected: 6,
};

const STATUS_EMOJI: Record<string, string> = {
  implemented: "✅",
  accepted: "🟢",
  proposed: "🟡",
  draft: "📝",
  superseded: "🔄",
  rejected: "❌",
};

const STATUS_DESCRIPTION: Record<string, string> = {
  implemented: "Feature is live in BAML - highest reference value",
  accepted: "Approved for implementation - mature design",
  proposed: "Ready for review - may still evolve",
  draft: "Work in progress - early stage",
  superseded: "Replaced by another BEP",
  rejected: "Decided against",
};

function getStatusEmoji(status: string): string {
  return STATUS_EMOJI[status] || "❓";
}

function getStatusDescription(status: string): string {
  return STATUS_DESCRIPTION[status] || "Unknown status";
}

function sortBepsByStatus(beps: ExportAllBep[]): ExportAllBep[] {
  return [...beps].sort((a, b) => {
    const orderA = STATUS_ORDER[a.status] ?? 99;
    const orderB = STATUS_ORDER[b.status] ?? 99;
    if (orderA !== orderB) return orderA - orderB;
    return a.number - b.number;
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// Formatting Utilities
// ─────────────────────────────────────────────────────────────────────────────

function formatBepNumber(num: number): string {
  return `BEP-${String(num).padStart(3, "0")}`;
}

function formatDate(timestamp: number): string {
  return new Date(timestamp).toISOString().split("T")[0];
}

function extractSummary(content: string, maxLength: number = 300): string {
  if (!content) return "No summary available.";

  // Remove the title (first # heading)
  let text = content.replace(/^#\s+.+\n+/, "");

  // Remove other headings for a cleaner summary
  text = text.replace(/^#{1,6}\s+.+$/gm, "");

  // Remove code blocks
  text = text.replace(/```[\s\S]*?```/g, "");

  // Remove inline code
  text = text.replace(/`[^`]+`/g, "");

  // Remove links but keep text
  text = text.replace(/\[([^\]]+)\]\([^)]+\)/g, "$1");

  // Remove bold/italic markers
  text = text.replace(/\*\*([^*]+)\*\*/g, "$1");
  text = text.replace(/\*([^*]+)\*/g, "$1");

  // Remove HTML comments
  text = text.replace(/<!--[\s\S]*?-->/g, "");

  // Collapse whitespace
  text = text.replace(/\s+/g, " ").trim();

  // Take first paragraph or truncate
  const firstPara = text.split(/\n\n/)[0] || text;

  if (firstPara.length <= maxLength) {
    return firstPara;
  }

  return firstPara.slice(0, maxLength - 3) + "...";
}

// ─────────────────────────────────────────────────────────────────────────────
// INDEX.md Generation
// ─────────────────────────────────────────────────────────────────────────────

export function generateIndexMd(data: ExportAllData): string {
  const sortedBeps = sortBepsByStatus(data.beps);

  let md = `# BAML Enhancement Proposals (BEPs) - Reference Bundle

> Exported: ${formatDate(data.exportedAt)}
> Total BEPs: ${data.beps.length}

This bundle contains all BEPs for reference when creating new proposals.
BEPs are sorted by maturity - **implemented and accepted BEPs are the best references**.

## Status Legend

| Status | Emoji | Meaning | Reference Value |
|--------|-------|---------|-----------------|
| Implemented | ${STATUS_EMOJI.implemented} | Feature is live in BAML | ⭐⭐⭐ Highest |
| Accepted | ${STATUS_EMOJI.accepted} | Approved for implementation | ⭐⭐⭐ High |
| Proposed | ${STATUS_EMOJI.proposed} | Ready for review | ⭐⭐ Medium |
| Draft | ${STATUS_EMOJI.draft} | Work in progress | ⭐ Lower |
| Superseded | ${STATUS_EMOJI.superseded} | Replaced by another BEP | Context only |
| Rejected | ${STATUS_EMOJI.rejected} | Decided against | Context only |

---

## All BEPs

`;

  // Group by status for better organization
  const byStatus: Record<string, ExportAllBep[]> = {};
  for (const bep of sortedBeps) {
    if (!byStatus[bep.status]) {
      byStatus[bep.status] = [];
    }
    byStatus[bep.status].push(bep);
  }

  // Output each status group
  const statusOrder = ["implemented", "accepted", "proposed", "draft", "superseded", "rejected"];

  for (const status of statusOrder) {
    const beps = byStatus[status];
    if (!beps || beps.length === 0) continue;

    md += `### ${getStatusEmoji(status)} ${status.charAt(0).toUpperCase() + status.slice(1)} (${beps.length})

> ${getStatusDescription(status)}

`;

    for (const bep of beps) {
      const bepNum = formatBepNumber(bep.number);
      const summary = extractSummary(bep.content, 150);
      const pageCount = bep.pages.length;
      const pageInfo = pageCount > 0 ? ` | ${pageCount} pages` : "";
      const issueInfo = bep.openIssueCount > 0 ? ` | ${bep.openIssueCount} open issues` : "";

      md += `- **[${bepNum}: ${bep.title}](./${bepNum}/README.md)** (v${bep.currentVersion}${pageInfo}${issueInfo})
  ${summary}
  *Shepherds: ${bep.shepherdNames.join(", ") || "None"}*

`;
    }
  }

  md += `---

## Quick Reference by Number

| BEP | Title | Status | Version | Shepherds |
|-----|-------|--------|---------|-----------|
`;

  // Sort by number for the quick reference table
  const byNumber = [...data.beps].sort((a, b) => a.number - b.number);
  for (const bep of byNumber) {
    const bepNum = formatBepNumber(bep.number);
    const emoji = getStatusEmoji(bep.status);
    md += `| [${bepNum}](./${bepNum}/README.md) | ${bep.title} | ${emoji} ${bep.status} | v${bep.currentVersion} | ${bep.shepherdNames.join(", ") || "-"} |\n`;
  }

  md += `
---

## Creating a New BEP

When creating a new BEP, consider referencing:
1. **Implemented/Accepted BEPs** - For proven patterns and structure
2. **BEPs in the same domain** - For consistency with related features
3. **Rejected BEPs** - To understand past decisions and avoid repeated mistakes
`;

  return md;
}


// ─────────────────────────────────────────────────────────────────────────────
// Individual BEP README.md Generation
// ─────────────────────────────────────────────────────────────────────────────

export function generateBepReadme(bep: ExportAllBep): string {
  const bepNum = formatBepNumber(bep.number);

  let md = `---
id: ${bepNum}
title: "${bep.title}"
status: ${bep.status}
version: ${bep.currentVersion}
shepherds: [${bep.shepherdNames.join(", ")}]
created: ${formatDate(bep.createdAt)}
updated: ${formatDate(bep.updatedAt)}
---

`;

  // Status badge at the top for quick reference
  md += `> **Status:** ${getStatusEmoji(bep.status)} **${bep.status.toUpperCase()}** - ${getStatusDescription(bep.status)}
`;

  if (bep.openIssueCount > 0) {
    md += `> **Open Issues:** ${bep.openIssueCount}
`;
  }

  md += `\n`;

  // Main content
  if (bep.content) {
    md += bep.content;
  } else {
    md += `# ${bepNum}: ${bep.title}\n\n*No content available.*\n`;
  }

  // Link to additional pages if any
  if (bep.pages.length > 0) {
    md += `\n\n---\n\n## Additional Pages\n\n`;
    for (const page of bep.pages) {
      md += `- [${page.title}](./pages/${page.slug}.md)\n`;
    }
  }

  return md;
}

// ─────────────────────────────────────────────────────────────────────────────
// Individual Page Generation
// ─────────────────────────────────────────────────────────────────────────────

export function generatePageMd(page: ExportAllPage): string {
  return `---
slug: ${page.slug}
title: "${page.title}"
---

${page.content}
`;
}

// ─────────────────────────────────────────────────────────────────────────────
// Generate All Files
// ─────────────────────────────────────────────────────────────────────────────

export function generateAllBepsExportFiles(data: ExportAllData): ExportAllFile[] {
  const files: ExportAllFile[] = [];

  // Index file
  files.push({
    path: "INDEX.md",
    content: generateIndexMd(data),
  });

  // Individual BEP folders
  for (const bep of data.beps) {
    const bepFolder = formatBepNumber(bep.number);

    // Main README
    files.push({
      path: `${bepFolder}/README.md`,
      content: generateBepReadme(bep),
    });

    // Additional pages
    for (const page of bep.pages) {
      files.push({
        path: `${bepFolder}/pages/${page.slug}.md`,
        content: generatePageMd(page),
      });
    }
  }

  return files;
}
