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
  isGoodReference: boolean;
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

/**
 * Generate a URL-safe slug from a title.
 * Converts to lowercase, replaces spaces with hyphens, removes special chars.
 */
function generateSlug(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, "") // Remove special characters
    .replace(/\s+/g, "-") // Replace spaces with hyphens
    .replace(/-+/g, "-") // Collapse multiple hyphens
    .replace(/^-|-$/g, "") // Remove leading/trailing hyphens
    .slice(0, 50); // Limit length
}

/**
 * Generate the folder name for a BEP: BEP-<NUMBER>-<slug>
 */
function formatBepFolderName(num: number, title: string): string {
  const bepNum = `BEP-${String(num).padStart(3, "0")}`;
  const slug = generateSlug(title);
  return slug ? `${bepNum}-${slug}` : bepNum;
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
      const bepFolder = formatBepFolderName(bep.number, bep.title);
      const summary = extractSummary(bep.content, 150);
      const pageCount = bep.pages.length;
      const pageInfo = pageCount > 0 ? ` | ${pageCount} pages` : "";
      const issueInfo = bep.openIssueCount > 0 ? ` | ${bep.openIssueCount} open issues` : "";
      const starBadge = bep.isGoodReference ? " ⭐" : "";

      md += `- **[${bepNum}: ${bep.title}](./${bepFolder}/Claude.md)**${starBadge} (v${bep.currentVersion}${pageInfo}${issueInfo})
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
    const bepFolder = formatBepFolderName(bep.number, bep.title);
    const emoji = getStatusEmoji(bep.status);
    const starBadge = bep.isGoodReference ? " ⭐" : "";
    md += `| [${bepNum}](./${bepFolder}/Claude.md)${starBadge} | ${bep.title} | ${emoji} ${bep.status} | v${bep.currentVersion} | ${bep.shepherdNames.join(", ") || "-"} |\n`;
  }

  md += `
---

## Creating a New BEP

See the \`NEW-BEP/\` folder for:
- Directory structure to follow
- API instructions for uploading your BEP

When creating a new BEP, consider referencing:
1. **Implemented/Accepted BEPs** - For proven patterns and structure
2. **BEPs in the same domain** - For consistency with related features
3. **Rejected BEPs** - To understand past decisions and avoid repeated mistakes
`;

  return md;
}


// ─────────────────────────────────────────────────────────────────────────────
// Individual BEP meta.json Generation
// ─────────────────────────────────────────────────────────────────────────────

export interface BepMetadata {
  id: string;
  number: number;
  title: string;
  status: string;
  version: number;
  shepherds: string[];
  created: string;
  updated: string;
  isGoodReference: boolean;
  openIssueCount: number;
  pages: Array<{
    slug: string;
    title: string;
    order: number;
  }>;
}

export function generateBepMetaJson(bep: ExportAllBep): string {
  const metadata: BepMetadata = {
    id: formatBepNumber(bep.number),
    number: bep.number,
    title: bep.title,
    status: bep.status,
    version: bep.currentVersion,
    shepherds: bep.shepherdNames,
    created: formatDate(bep.createdAt),
    updated: formatDate(bep.updatedAt),
    isGoodReference: bep.isGoodReference,
    openIssueCount: bep.openIssueCount,
    pages: bep.pages.map((p) => ({
      slug: p.slug,
      title: p.title,
      order: p.order,
    })),
  };
  return JSON.stringify(metadata, null, 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// Individual BEP Claude.md Generation (main content file)
// ─────────────────────────────────────────────────────────────────────────────

export function generateBepClaudeMd(bep: ExportAllBep): string {
  const bepNum = formatBepNumber(bep.number);

  let md = "";

  // Good reference badge
  if (bep.isGoodReference) {
    md += `> ⭐ **Good Reference** - This BEP is marked as an excellent example for writing style.

`;
  }

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
  // No frontmatter - metadata is in meta.json
  return page.content;
}

// ─────────────────────────────────────────────────────────────────────────────
// NEW-BEP Instructions Generation
// ─────────────────────────────────────────────────────────────────────────────

export function generateNewBepInstructions(
  nextNumber: number,
  apiBaseUrl: string,
  goodReferenceBeps: ExportAllBep[] = []
): string {
  const bepNum = formatBepNumber(nextNumber);

  let md = `# Creating a New BEP

Next available BEP number: **${nextNumber}** (${bepNum})

`;

  if (goodReferenceBeps.length > 0) {
    md += `## Good Reference BEPs

These BEPs are excellent examples of writing style and structure:

`;
    for (const bep of goodReferenceBeps) {
      const refBepNum = formatBepNumber(bep.number);
      const refFolder = formatBepFolderName(bep.number, bep.title);
      const summary = extractSummary(bep.content, 120);
      md += `- **[${refBepNum}: ${bep.title}](../${refFolder}/Claude.md)** (${getStatusEmoji(bep.status)} ${bep.status})
  ${summary}

`;
    }
    md += `---

`;
  }

  md += `## Directory Structure

Each BEP is exported as a folder with this structure:

\`\`\`
BEP-001-your-proposal-slug/
├── meta.json           # Metadata (status, version, shepherds, pages list)
├── Claude.md           # Main proposal content
└── pages/              # Additional pages (addenda)
    ├── background.md
    └── examples.md
\`\`\`

---

## Writing Style

Write in the style of a [PEP](https://peps.python.org/) or [TC39 proposal](https://github.com/tc39/proposals).

A good BEP is a **single document** with this structure:

\`\`\`markdown
# Title

## Summary

Brief description + code example showing the feature in action.

## Prior Art (optional)

Research, other languages, related work.

## Proposed Design

The detailed technical design. This is the heart of the BEP.

## Design Tradeoffs

What alternatives were considered? Why this approach?

## Open Questions

Unresolved decisions, future work.
\`\`\`

---

## Getting Your API Token

**Only BoundaryML team members can create BEPs via the API.**

1. Go to your profile page: ${apiBaseUrl}/profile
2. Click "Generate API Token"
3. Copy the token (starts with \`bep_\`)

---

## API Usage

### Create a New BEP

\`\`\`bash
curl -X POST "${apiBaseUrl}/api/agent/beps" \\
  -H "Authorization: Bearer <your-api-token>" \\
  -H "Content-Type: application/json" \\
  -d '{
    "title": "Your Proposal Title",
    "content": "# Your Proposal Title\\n\\n## Summary\\n\\n..."
  }'
\`\`\`

Response:
\`\`\`json
{
  "success": true,
  "number": ${nextNumber},
  "formattedId": "${bepNum}",
  "createdBy": "Your Name",
  "url": "${apiBaseUrl}/beps/${nextNumber}"
}
\`\`\`

### Create a BEP with Additional Pages (Addenda)

You can include multiple pages in a single API call:

\`\`\`bash
curl -X POST "${apiBaseUrl}/api/agent/beps" \\
  -H "Authorization: Bearer <your-api-token>" \\
  -H "Content-Type: application/json" \\
  -d '{
    "title": "Your Proposal Title",
    "content": "# Main Content\\n\\n...",
    "pages": [
      {
        "slug": "background",
        "title": "Background Research",
        "content": "# Background\\n\\nDetailed research..."
      },
      {
        "slug": "examples",
        "title": "Code Examples",
        "content": "# Examples\\n\\nUsage examples..."
      }
    ]
  }'
\`\`\`

### Update an Existing BEP

\`\`\`bash
curl -X PUT "${apiBaseUrl}/api/agent/beps" \\
  -H "Authorization: Bearer <your-api-token>" \\
  -H "Content-Type: application/json" \\
  -d '{
    "number": ${nextNumber},
    "content": "# Updated Title\\n\\n...",
    "editNote": "Updated based on feedback",
    "versionMode": "new"
  }'
\`\`\`

### Update with Additional Pages

When updating, you can add, update, or remove pages:

\`\`\`bash
curl -X PUT "${apiBaseUrl}/api/agent/beps" \\
  -H "Authorization: Bearer <your-api-token>" \\
  -H "Content-Type: application/json" \\
  -d '{
    "number": ${nextNumber},
    "content": "# Updated Main Content\\n\\n...",
    "pages": [
      {
        "slug": "background",
        "title": "Updated Background",
        "content": "# Background\\n\\nUpdated research..."
      },
      {
        "slug": "new-addendum",
        "title": "New Addendum",
        "content": "# New Section\\n\\nNew content..."
      }
    ],
    "editNote": "Added new addendum, updated background",
    "versionMode": "new"
  }'
\`\`\`

**Note:** When you provide a \`pages\` array in an update:
- Pages with matching slugs are **updated**
- Pages with new slugs are **created**
- Pages that exist but aren't in the array are **deleted**

To keep existing pages unchanged, omit the \`pages\` field entirely.

---

## API Reference

### Create (POST /api/agent/beps)

| Field | Required | Description |
|-------|----------|-------------|
| \`title\` | Yes | The BEP title |
| \`content\` | Yes | Full markdown content for Claude.md |
| \`pages\` | No | Array of additional pages (addenda) |

**Page object:**

| Field | Required | Description |
|-------|----------|-------------|
| \`slug\` | Yes | URL-safe identifier (e.g., \`"background"\`, \`"examples"\`) |
| \`title\` | Yes | Display title for the page |
| \`content\` | Yes | Full markdown content |

### Update (PUT /api/agent/beps)

| Field | Required | Description |
|-------|----------|-------------|
| \`number\` | Yes | The BEP number to update |
| \`title\` | No | Updated title |
| \`content\` | No* | Updated markdown content |
| \`pages\` | No* | Updated pages array (replaces all existing pages) |
| \`editNote\` | No | Note describing the changes |
| \`versionMode\` | No | \`"new"\` (default) creates a new version, \`"current"\` updates in place |

*At least one of \`title\`, \`content\`, or \`pages\` must be provided.

### Response Fields

| Field | Description |
|-------|-------------|
| \`success\` | \`true\` if operation succeeded |
| \`bepId\` | Internal BEP ID |
| \`number\` | BEP number |
| \`formattedId\` | Formatted ID (e.g., "BEP-001") |
| \`url\` | URL to view the BEP |
| \`versionNumber\` | (update only) New version number |
| \`versionAction\` | (update only) \`"created"\` or \`"updated"\` |
| \`pagesCreated\` | (update only) Number of pages created |
| \`pagesUpdated\` | (update only) Number of pages updated |
| \`pagesDeleted\` | (update only) Number of pages deleted |
`;

  return md;
}

// ─────────────────────────────────────────────────────────────────────────────
// Generate All Files
// ─────────────────────────────────────────────────────────────────────────────

export function generateAllBepsExportFiles(data: ExportAllData, apiBaseUrl: string = "https://beps.boundaryml.com"): ExportAllFile[] {
  const files: ExportAllFile[] = [];

  // Calculate next BEP number
  const maxNumber = data.beps.reduce((max, bep) => Math.max(max, bep.number), 0);
  const nextNumber = maxNumber + 1;

  // Find good reference BEPs (sorted by status priority, then number)
  const goodReferenceBeps = sortBepsByStatus(
    data.beps.filter((bep) => bep.isGoodReference)
  );

  // Index file
  files.push({
    path: "INDEX.md",
    content: generateIndexMd(data),
  });

  // NEW-BEP instructions
  files.push({
    path: "NEW-BEP/INSTRUCTIONS.md",
    content: generateNewBepInstructions(nextNumber, apiBaseUrl, goodReferenceBeps),
  });

  // Individual BEP folders: BEP-<NUMBER>-<slug>/
  for (const bep of data.beps) {
    const bepFolder = formatBepFolderName(bep.number, bep.title);

    // Metadata JSON file
    files.push({
      path: `${bepFolder}/meta.json`,
      content: generateBepMetaJson(bep),
    });

    // Main content file (Claude.md)
    files.push({
      path: `${bepFolder}/Claude.md`,
      content: generateBepClaudeMd(bep),
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
