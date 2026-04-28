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
// Claude.md Generation (main index file)
// ─────────────────────────────────────────────────────────────────────────────

export function generateClaudeMd(data: ExportAllData): string {
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

      md += `- **[${bepNum}: ${bep.title}](./${bepFolder}/README.md)**${starBadge} (v${bep.currentVersion}${pageInfo}${issueInfo})
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
    md += `| [${bepNum}](./${bepFolder}/README.md)${starBadge} | ${bep.title} | ${emoji} ${bep.status} | v${bep.currentVersion} | ${bep.shepherdNames.join(", ") || "-"} |\n`;
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
// Individual BEP README.md Generation (main content file)
// ─────────────────────────────────────────────────────────────────────────────

export function generateBepReadme(bep: ExportAllBep): string {
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
      md += `- **[${refBepNum}: ${bep.title}](../${refFolder}/README.md)** (${getStatusEmoji(bep.status)} ${bep.status})
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
├── README.md           # Main proposal content
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

## Pull API - Download All BEPs

Download all BEPs as a ZIP archive:

\`\`\`bash
# Download to a new timestamped folder (copy mode - default)
curl -o all-beps.zip "${apiBaseUrl}/api/agent/beps/pull"
unzip all-beps.zip -d all-beps-$(date +%Y%m%d)

# Download and replace existing folder (inplace mode)
curl -o all-beps.zip "${apiBaseUrl}/api/agent/beps/pull"
rm -rf ./all-beps && unzip all-beps.zip -d ./all-beps
\`\`\`

The ZIP contains the full export structure:
\`\`\`
all-beps/
├── Claude.md                    # Main index
├── NEW-BEP/INSTRUCTIONS.md      # This file
├── BEP-001-slug/
│   ├── meta.json
│   ├── README.md
│   └── pages/
└── ...
\`\`\`

---

## Push API - Create/Update BEPs

### Create a New BEP

\`\`\`bash
curl -X POST "${apiBaseUrl}/api/agent/beps" \\
  -H "Authorization: Bearer <your-api-token>" \\
  -H "Content-Type: application/json" \\
  -d '{
    "title": "Your Proposal Title",
    "content": "# Your Proposal Title\\n\\n## Summary\\n\\n...",
    "pages": [
      {
        "slug": "background",
        "title": "Background Research",
        "content": "# Background\\n\\nDetailed research..."
      }
    ]
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

### Update an Existing BEP

\`\`\`bash
curl -X PUT "${apiBaseUrl}/api/agent/beps" \\
  -H "Authorization: Bearer <your-api-token>" \\
  -H "Content-Type: application/json" \\
  -d '{
    "number": ${nextNumber},
    "content": "# Updated Title\\n\\n...",
    "pages": [
      {
        "slug": "background",
        "title": "Updated Background",
        "content": "# Background\\n\\nUpdated..."
      },
      {
        "slug": "new-page",
        "title": "New Page",
        "content": "# New\\n\\n..."
      }
    ],
    "editNote": "Updated based on feedback",
    "versionMode": "new"
  }'
\`\`\`

**Page behavior on update:**
- Pages with matching slugs → **updated**
- Pages with new slugs → **created**
- Existing pages not in array → **deleted**
- Omit \`pages\` field → keep existing pages unchanged

---

## API Reference

### Pull (GET /api/agent/beps/pull)

Downloads all BEPs as a ZIP archive. No authentication required.

| Param | Type | Description |
|-------|------|-------------|
| (none) | | Returns ZIP file |

### Create (POST /api/agent/beps)

| Field | Required | Description |
|-------|----------|-------------|
| \`title\` | Yes | The BEP title |
| \`content\` | Yes | Full markdown content |
| \`pages\` | No | Array of additional pages |

**Page object:**

| Field | Required | Description |
|-------|----------|-------------|
| \`slug\` | Yes | URL-safe identifier (e.g., \`"background"\`) |
| \`title\` | Yes | Display title |
| \`content\` | Yes | Full markdown content |

### Update (PUT /api/agent/beps)

| Field | Required | Description |
|-------|----------|-------------|
| \`number\` | Yes | The BEP number to update |
| \`title\` | No | Updated title |
| \`content\` | No* | Updated markdown content |
| \`pages\` | No* | Updated pages array (replaces all existing) |
| \`editNote\` | No | Note describing the changes |
| \`versionMode\` | No | \`"new"\` (default) or \`"current"\` |

*At least one of \`title\`, \`content\`, or \`pages\` must be provided.
`;

  return md;
}

// ─────────────────────────────────────────────────────────────────────────────
// BEP Skill Generation (for Claude Code)
// ─────────────────────────────────────────────────────────────────────────────

export function generateBepSkill(): string {
  return `---
name: bep
description: Work with BAML Enhancement Proposals — create, edit, review, or push BEPs.
user-invocable: true
allowed-tools: Bash Read Edit Write Grep Glob Agent
argument-hint: [number-or-title]
---

You are working in a BEP (BAML Enhancement Proposal) repository. Use the \`./bep\` CLI for all operations.

## CLI Reference

\`\`\`
./bep pull              # Fetch all BEPs from server, diff, prompt to apply
./bep list              # List local BEPs (--status STATUS, --json)
./bep new "Title"       # Scaffold BEP-?-slug/ locally
./bep diff <ref>        # Diff local vs server (--full for unified diff)
./bep push <ref>        # Dry-run diff + prompt to push (-y to confirm)
./bep open <number>     # Open in browser
\`\`\`

\`<ref>\` is a BEP number (e.g. \`41\`) or slug (e.g. \`my-feature\` for \`BEP-?-my-feature\`).

## What to do

If \`$ARGUMENTS\` is a number, read that BEP's README.md and ask what the user wants to change.

If \`$ARGUMENTS\` is a title/topic, create a new BEP:
1. Run \`./bep new "$ARGUMENTS"\`
2. Read good reference BEPs (marked with \`isGoodReference\` in meta.json) for style
3. Write the README.md following the structure below
4. Run \`./bep diff <slug>\` to preview, then ask if user wants to push

If no arguments, ask what the user wants to do.

## BEP Writing Style

Write like a [PEP](https://peps.python.org/) or [TC39 proposal](https://github.com/tc39/proposals).

Structure:

\`\`\`markdown
# Title

## Summary

Brief description + code example showing the feature.

## Prior Art

Other languages, related work.

## Proposed Design

Detailed technical design — the heart of the BEP.

## Design Tradeoffs

Alternatives considered and why this approach wins.

## Open Questions

Unresolved decisions, future work.
\`\`\`

Be concrete. Lead with code examples. Avoid hand-waving.
`;
}

// ─────────────────────────────────────────────────────────────────────────────
// BEP CLI Script Generation
// ─────────────────────────────────────────────────────────────────────────────

export function generateBepScript(apiBaseUrl: string): string {
  return `#!/usr/bin/env -S uv run --script
# /// script
# dependencies = ["httpx"]
# ///
"""
BEP CLI - Command-line tool for working with BAML Enhancement Proposals.

Usage:
    ./bep pull              # Fetch all BEPs from server into current directory
    ./bep list              # List local BEPs (--status STATUS, --json)
    ./bep new "Title"       # Scaffold a new BEP-?-slug/ locally
    ./bep diff <ref>        # Diff local vs server (--full for unified diff)
    ./bep push <ref>        # Dry-run diff + prompt to push (-y to confirm)
    ./bep open <number>     # Open BEP in browser
"""

import argparse
import json
import os
import re
import subprocess
import sys
import webbrowser
import zipfile
from datetime import date
from io import BytesIO
from pathlib import Path

# ─────────────────────────────────────────────────────────────────────────────
# Configuration
# ─────────────────────────────────────────────────────────────────────────────

SCRIPT_DIR = Path(__file__).resolve().parent
ALL_BEPS_DIR = SCRIPT_DIR  # BEPs are in the same directory as this script

# API endpoints
DEFAULT_API_BASE = "${apiBaseUrl}"
API_BASE = os.environ.get("BEP_API_BASE", DEFAULT_API_BASE)
PULL_ENDPOINT = f"{API_BASE}/api/agent/beps/pull"
PUSH_ENDPOINT = f"{API_BASE}/api/agent/beps"

# Token for push operations (read from env)
API_TOKEN = os.environ.get("BEP_API_TOKEN", "")


# ─────────────────────────────────────────────────────────────────────────────
# Helpers
# ─────────────────────────────────────────────────────────────────────────────

def format_bep_number(num: int) -> str:
    return f"BEP-{num:03d}"


def parse_bep_ref(ref: str) -> tuple[int | None, str | None]:
    """
    Parse a BEP reference: number (41) or slug (my-feature for BEP-?-my-feature).
    Returns (number, slug) where one is None.
    """
    ref = ref.strip()
    
    # Try to parse as number
    if ref.isdigit():
        return int(ref), None
    
    # Try BEP-XXX format
    m = re.match(r"(?:BEP-)?(\\d+)", ref, re.IGNORECASE)
    if m:
        return int(m.group(1)), None
    
    # It's a slug
    return None, ref


def find_local_bep(ref: str) -> Path | None:
    """Find a local BEP folder by number or slug."""
    num, slug = parse_bep_ref(ref)
    
    if not ALL_BEPS_DIR.exists():
        return None
    
    for folder in ALL_BEPS_DIR.iterdir():
        if not folder.is_dir() or not folder.name.startswith("BEP-"):
            continue
        
        # Parse folder name: BEP-XXX-slug
        m = re.match(r"BEP-(\\d+|\\?)(?:-(.+))?", folder.name)
        if not m:
            continue
        
        folder_num_str = m.group(1)
        folder_num = int(folder_num_str) if folder_num_str != "?" else None
        folder_slug = m.group(2) or ""
        
        if num is not None and folder_num == num:
            return folder
        if slug is not None and slug.lower() in folder_slug.lower():
            return folder
        if slug is not None and slug.lower() in folder.name.lower():
            return folder
    
    return None


def get_git_author() -> str:
    """Get git author name and email."""
    try:
        name = subprocess.check_output(
            ["git", "config", "user.name"], text=True
        ).strip()
        email = subprocess.check_output(
            ["git", "config", "user.email"], text=True
        ).strip()
        if name and email:
            return f"{name} <{email}>"
        return name or "<Author>"
    except Exception:
        return "<Author>"


def slugify(text: str) -> str:
    """Convert text to URL-safe slug."""
    text = text.lower()
    text = re.sub(r"[^a-z0-9]+", "-", text)
    return text.strip("-")[:50]


def read_local_bep(folder: Path) -> dict:
    """Read a local BEP folder and return its content."""
    result = {
        "folder": folder.name,
        "readme": "",
        "pages": [],
        "meta": None,
    }
    
    readme_path = folder / "README.md"
    if readme_path.exists():
        result["readme"] = readme_path.read_text(encoding="utf-8")
    
    meta_path = folder / "meta.json"
    if meta_path.exists():
        result["meta"] = json.loads(meta_path.read_text(encoding="utf-8"))
    
    pages_dir = folder / "pages"
    if pages_dir.exists() and pages_dir.is_dir():
        for page_file in sorted(pages_dir.glob("*.md")):
            result["pages"].append({
                "slug": page_file.stem,
                "content": page_file.read_text(encoding="utf-8"),
            })
    
    return result


# ─────────────────────────────────────────────────────────────────────────────
# Commands
# ─────────────────────────────────────────────────────────────────────────────

def cmd_pull(args: argparse.Namespace) -> int:
    """Fetch all BEPs from server and extract to current directory."""
    import httpx
    
    print(f"Pulling BEPs from {PULL_ENDPOINT}...")
    
    try:
        response = httpx.get(PULL_ENDPOINT, timeout=60.0)
        response.raise_for_status()
    except httpx.HTTPError as e:
        print(f"Error: Failed to fetch BEPs: {e}", file=sys.stderr)
        return 1
    
    # Extract ZIP
    zip_data = BytesIO(response.content)
    
    with zipfile.ZipFile(zip_data, "r") as zf:
        # First, show what would change
        existing_files = set()
        for f in ALL_BEPS_DIR.rglob("*"):
            if f.is_file() and f.name != "bep":  # Don't count the script itself
                existing_files.add(f.relative_to(ALL_BEPS_DIR))
        
        new_files = set()
        for name in zf.namelist():
            # Strip "all-beps/" prefix if present
            if name.startswith("all-beps/"):
                rel_path = name[len("all-beps/"):]
            else:
                rel_path = name
            if rel_path and not rel_path.endswith("/"):
                new_files.add(Path(rel_path))
        
        added = new_files - existing_files
        removed = existing_files - new_files
        updated = new_files & existing_files
        
        if added or removed:
            print(f"\\nChanges:")
            if added:
                print(f"  + {len(added)} new files")
                for f in sorted(added)[:5]:
                    print(f"    + {f}")
                if len(added) > 5:
                    print(f"    ... and {len(added) - 5} more")
            if removed:
                print(f"  - {len(removed)} files to remove")
                for f in sorted(removed)[:5]:
                    print(f"    - {f}")
                if len(removed) > 5:
                    print(f"    ... and {len(removed) - 5} more")
            if updated:
                print(f"  ~ {len(updated)} files to update")
        else:
            print("\\nNo changes detected.")
            if not args.force:
                return 0
        
        if not args.yes:
            response_input = input("\\nApply changes? [y/N]: ").strip().lower()
            if response_input not in ("y", "yes"):
                print("Aborted.")
                return 0
        
        # Clear existing BEP folders and extract
        import shutil
        for item in ALL_BEPS_DIR.iterdir():
            # Keep the bep script and skills folder
            if item.name in ("bep", "skills"):
                continue
            if item.is_dir() and item.name.startswith("BEP-"):
                shutil.rmtree(item)
            elif item.is_dir() and item.name == "NEW-BEP":
                shutil.rmtree(item)
            elif item.name in ("Claude.md",):
                item.unlink()
        
        # Extract
        for name in zf.namelist():
            if name.startswith("all-beps/"):
                rel_path = name[len("all-beps/"):]
            else:
                rel_path = name
            
            if not rel_path or rel_path.endswith("/"):
                continue
            
            # Skip the bep script from the archive (we already have it)
            if rel_path == "bep":
                continue
            
            target_path = ALL_BEPS_DIR / rel_path
            target_path.parent.mkdir(parents=True, exist_ok=True)
            target_path.write_bytes(zf.read(name))
        
        print(f"\\nExtracted to {ALL_BEPS_DIR}")
        return 0


def cmd_list(args: argparse.Namespace) -> int:
    """List local BEPs."""
    beps = []
    for folder in sorted(ALL_BEPS_DIR.iterdir()):
        if not folder.is_dir() or not folder.name.startswith("BEP-"):
            continue
        
        meta_path = folder / "meta.json"
        if not meta_path.exists():
            continue
        
        try:
            meta = json.loads(meta_path.read_text(encoding="utf-8"))
            beps.append({
                "folder": folder.name,
                "number": meta.get("number"),
                "title": meta.get("title"),
                "status": meta.get("status", "unknown").lower(),
                "version": meta.get("version"),
                "isGoodReference": meta.get("isGoodReference", False),
            })
        except Exception:
            continue
    
    # Filter by status if requested
    if args.status:
        status_filter = args.status.lower()
        beps = [b for b in beps if b["status"] == status_filter]
    
    if args.json:
        print(json.dumps(beps, indent=2))
    else:
        if not beps:
            print("No BEPs found. Run './bep pull' first.")
            return 0
        
        print(f"{'ID':<12} {'Status':<12} {'Title'}")
        print("-" * 60)
        for b in beps:
            star = "⭐ " if b.get("isGoodReference") else ""
            num = b['number']
            if num is not None:
                print(f"BEP-{num:03d}     {b['status']:<12} {star}{b['title']}")
            else:
                print(f"BEP-???     {b['status']:<12} {star}{b['title']}")
    
    return 0


def cmd_new(args: argparse.Namespace) -> int:
    """Scaffold a new BEP locally."""
    title = args.title
    author = args.author or get_git_author()
    
    # Create with unknown number (BEP-?-slug)
    slug = slugify(title)
    folder_name = f"BEP-?-{slug}"
    folder_path = ALL_BEPS_DIR / folder_name
    
    if folder_path.exists():
        print(f"Error: {folder_path} already exists.", file=sys.stderr)
        return 1
    
    folder_path.mkdir(parents=True)
    
    # Create meta.json (partial, will be filled on push)
    meta = {
        "id": "BEP-???",
        "number": None,
        "title": title,
        "status": "draft",
        "version": 1,
        "shepherds": [author.split(" <")[0] if " <" in author else author],
        "created": date.today().isoformat(),
        "updated": date.today().isoformat(),
        "isGoodReference": False,
        "openIssueCount": 0,
        "pages": [],
    }
    (folder_path / "meta.json").write_text(json.dumps(meta, indent=2), encoding="utf-8")
    
    # Create README.md with template
    readme_content = f"""# {title}

## Summary

A concise explanation (3–8 sentences) of what this proposal does and what it enables for BAML users.

\`\`\`baml
// Example code showing the feature
\`\`\`

## Prior Art

Research, other languages, related work.

## Proposed Design

The detailed technical design. This is the heart of the BEP.

### Syntax

\`\`\`baml
// Example BAML syntax
\`\`\`

### Semantics

How the compiler/runtime handles this.

## Design Tradeoffs

What alternatives were considered? Why this approach?

## Open Questions

Unresolved decisions, future work.
"""
    
    (folder_path / "README.md").write_text(readme_content, encoding="utf-8")
    
    # Create empty pages directory
    (folder_path / "pages").mkdir()
    
    print(f"Created {folder_path}")
    print(f"\\nNext steps:")
    print(f"  1. Edit {folder_path}/README.md")
    print(f"  2. Run: ./bep diff {slug}")
    print(f"  3. Run: ./bep push {slug}")
    
    return 0


def cmd_diff(args: argparse.Namespace) -> int:
    """Show diff between local and server."""
    import httpx
    
    ref = args.ref
    local_folder = find_local_bep(ref)
    
    if not local_folder:
        print(f"Error: Could not find local BEP matching '{ref}'.", file=sys.stderr)
        print("Available BEPs:")
        cmd_list(argparse.Namespace(status=None, json=False))
        return 1
    
    local_bep = read_local_bep(local_folder)
    meta = local_bep.get("meta") or {}
    bep_number = meta.get("number")
    
    # For new BEPs (BEP-?-*), there's nothing to diff against
    if bep_number is None:
        print(f"New BEP: {local_folder.name}")
        print("\\nContent to be created:")
        print("-" * 40)
        if args.full:
            print(local_bep["readme"])
        else:
            lines = local_bep["readme"].split("\\n")[:20]
            print("\\n".join(lines))
            if len(local_bep["readme"].split("\\n")) > 20:
                print(f"... ({len(local_bep['readme'].split(chr(10)))} total lines)")
        
        if local_bep["pages"]:
            print(f"\\nPages: {len(local_bep['pages'])}")
            for page in local_bep["pages"]:
                print(f"  - {page['slug']}.md")
        
        return 0
    
    # Fetch current server version
    print(f"Fetching {format_bep_number(bep_number)} from server...")
    
    try:
        response = httpx.get(
            PUSH_ENDPOINT,
            params={"name": str(bep_number), "omitOtherVersions": "true"},
            timeout=30.0,
        )
        response.raise_for_status()
        server_data = response.json()
    except httpx.HTTPError as e:
        print(f"Error: Failed to fetch from server: {e}", file=sys.stderr)
        return 1
    
    # Extract server content
    server_files = {f["path"]: f["content"] for f in server_data.get("files", [])}
    server_readme = server_files.get("README.md", "")
    
    # Compare
    import difflib
    
    local_readme = local_bep["readme"]
    
    if args.full:
        diff = difflib.unified_diff(
            server_readme.splitlines(keepends=True),
            local_readme.splitlines(keepends=True),
            fromfile=f"server/{format_bep_number(bep_number)}/README.md",
            tofile=f"local/{local_folder.name}/README.md",
        )
        diff_text = "".join(diff)
        if diff_text:
            print(diff_text)
        else:
            print("No differences in README.md")
    else:
        # Summary diff
        server_lines = len(server_readme.splitlines())
        local_lines = len(local_readme.splitlines())
        
        if server_readme == local_readme:
            print("README.md: No changes")
        else:
            print(f"README.md: {server_lines} → {local_lines} lines")
    
    # Compare pages
    server_pages = {}
    for f_path, content in server_files.items():
        if f_path.startswith("pages/") and f_path.endswith(".md"):
            slug = f_path[6:-3]  # Remove "pages/" and ".md"
            server_pages[slug] = content
    
    local_pages = {p["slug"]: p["content"] for p in local_bep["pages"]}
    
    all_slugs = set(server_pages.keys()) | set(local_pages.keys())
    for slug in sorted(all_slugs):
        server_content = server_pages.get(slug, "")
        local_content = local_pages.get(slug, "")
        
        if slug not in server_pages:
            print(f"pages/{slug}.md: NEW ({len(local_content.splitlines())} lines)")
        elif slug not in local_pages:
            print(f"pages/{slug}.md: DELETED")
        elif server_content != local_content:
            print(f"pages/{slug}.md: MODIFIED")
            if args.full:
                diff = difflib.unified_diff(
                    server_content.splitlines(keepends=True),
                    local_content.splitlines(keepends=True),
                    fromfile=f"server/pages/{slug}.md",
                    tofile=f"local/pages/{slug}.md",
                )
                print("".join(diff))
    
    return 0


def cmd_push(args: argparse.Namespace) -> int:
    """Push local BEP to server."""
    import httpx
    
    if not API_TOKEN:
        print("Error: BEP_API_TOKEN environment variable not set.", file=sys.stderr)
        print("Get your token from: ${apiBaseUrl}/profile", file=sys.stderr)
        return 1
    
    ref = args.ref
    local_folder = find_local_bep(ref)
    
    if not local_folder:
        print(f"Error: Could not find local BEP matching '{ref}'.", file=sys.stderr)
        return 1
    
    local_bep = read_local_bep(local_folder)
    meta = local_bep.get("meta") or {}
    bep_number = meta.get("number")
    
    # Prepare payload
    readme_content = local_bep["readme"]
    pages = []
    for page in local_bep["pages"]:
        # Extract title from first heading or use slug
        title_match = re.search(r"^#\\s+(.+)$", page["content"], re.MULTILINE)
        title = title_match.group(1).strip() if title_match else page["slug"].title()
        pages.append({
            "slug": page["slug"],
            "title": title,
            "content": page["content"],
        })
    
    # Show diff first
    print("Changes to push:")
    print("-" * 40)
    
    if bep_number is None:
        # New BEP
        print(f"CREATE new BEP: {meta.get('title', 'Untitled')}")
        print(f"  README.md: {len(readme_content.splitlines())} lines")
        if pages:
            print(f"  Pages: {len(pages)}")
            for p in pages:
                print(f"    - {p['slug']}.md ({len(p['content'].splitlines())} lines)")
    else:
        # Update existing
        print(f"UPDATE {format_bep_number(bep_number)}: {meta.get('title', '')}")
        # Run diff to show changes
        cmd_diff(argparse.Namespace(ref=ref, full=False))
    
    print()
    
    if not args.yes:
        response_input = input("Push these changes? [y/N]: ").strip().lower()
        if response_input not in ("y", "yes"):
            print("Aborted.")
            return 0
    
    headers = {
        "Authorization": f"Bearer {API_TOKEN}",
        "Content-Type": "application/json",
    }
    
    if bep_number is None:
        # Create new BEP
        payload = {
            "title": meta.get("title", "Untitled"),
            "content": readme_content,
        }
        if pages:
            payload["pages"] = pages
        
        try:
            response = httpx.post(
                PUSH_ENDPOINT,
                headers=headers,
                json=payload,
                timeout=30.0,
            )
            response.raise_for_status()
            result = response.json()
            
            print(f"\\n✓ Created {result.get('formattedId', 'BEP')}")
            print(f"  URL: {result.get('url', '')}")
            
            # Update local meta.json with the new number
            new_number = result.get("number")
            if new_number:
                meta["number"] = new_number
                meta["id"] = format_bep_number(new_number)
                (local_folder / "meta.json").write_text(
                    json.dumps(meta, indent=2), encoding="utf-8"
                )
                
                # Rename folder
                new_folder_name = f"BEP-{new_number:03d}-{slugify(meta.get('title', ''))}"
                new_folder_path = ALL_BEPS_DIR / new_folder_name
                local_folder.rename(new_folder_path)
                print(f"  Renamed to: {new_folder_name}")
            
        except httpx.HTTPError as e:
            print(f"Error: Failed to create BEP: {e}", file=sys.stderr)
            if hasattr(e, "response") and e.response is not None:
                try:
                    print(f"  {e.response.json()}", file=sys.stderr)
                except Exception:
                    pass
            return 1
    else:
        # Update existing BEP
        payload = {
            "number": bep_number,
            "content": readme_content,
            "editNote": args.note or "Updated via CLI",
            "versionMode": "new" if args.new_version else "current",
        }
        if pages:
            payload["pages"] = pages
        
        try:
            response = httpx.put(
                PUSH_ENDPOINT,
                headers=headers,
                json=payload,
                timeout=30.0,
            )
            response.raise_for_status()
            result = response.json()
            
            print(f"\\n✓ Updated {format_bep_number(bep_number)}")
            print(f"  Version: {result.get('versionNumber', '?')} ({result.get('versionAction', '')})")
            if result.get("pagesCreated"):
                print(f"  Pages created: {result['pagesCreated']}")
            if result.get("pagesUpdated"):
                print(f"  Pages updated: {result['pagesUpdated']}")
            if result.get("pagesDeleted"):
                print(f"  Pages deleted: {result['pagesDeleted']}")
            print(f"  URL: {result.get('url', '')}")
            
        except httpx.HTTPError as e:
            print(f"Error: Failed to update BEP: {e}", file=sys.stderr)
            if hasattr(e, "response") and e.response is not None:
                try:
                    print(f"  {e.response.json()}", file=sys.stderr)
                except Exception:
                    pass
            return 1
    
    return 0


def cmd_open(args: argparse.Namespace) -> int:
    """Open a BEP in the browser."""
    num, slug = parse_bep_ref(args.ref)
    
    if num is not None:
        url = f"{API_BASE}/beps/{num}"
    else:
        # Try to find the local BEP to get the number
        local_folder = find_local_bep(args.ref)
        if local_folder:
            meta_path = local_folder / "meta.json"
            if meta_path.exists():
                meta = json.loads(meta_path.read_text(encoding="utf-8"))
                num = meta.get("number")
                if num:
                    url = f"{API_BASE}/beps/{num}"
                else:
                    print(f"Error: BEP has no number assigned yet.", file=sys.stderr)
                    return 1
            else:
                print(f"Error: No meta.json found for {args.ref}.", file=sys.stderr)
                return 1
        else:
            print(f"Error: Could not find BEP matching '{args.ref}'.", file=sys.stderr)
            return 1
    
    print(f"Opening {url}")
    webbrowser.open(url)
    return 0


# ─────────────────────────────────────────────────────────────────────────────
# Main
# ─────────────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="BEP CLI - Work with BAML Enhancement Proposals",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    subparsers = parser.add_subparsers(dest="command", help="Available commands")
    
    # pull
    pull_parser = subparsers.add_parser("pull", help="Fetch all BEPs from server")
    pull_parser.add_argument("-y", "--yes", action="store_true", help="Skip confirmation")
    pull_parser.add_argument("-f", "--force", action="store_true", help="Force even if no changes")
    
    # list
    list_parser = subparsers.add_parser("list", help="List local BEPs")
    list_parser.add_argument("--status", help="Filter by status (draft, proposed, accepted, etc.)")
    list_parser.add_argument("--json", action="store_true", help="Output as JSON")
    
    # new
    new_parser = subparsers.add_parser("new", help="Scaffold a new BEP")
    new_parser.add_argument("title", help="Title of the proposal")
    new_parser.add_argument("--author", help="Author name")
    
    # diff
    diff_parser = subparsers.add_parser("diff", help="Diff local vs server")
    diff_parser.add_argument("ref", help="BEP number (e.g., 41) or slug (e.g., my-feature)")
    diff_parser.add_argument("--full", action="store_true", help="Show full unified diff")
    
    # push
    push_parser = subparsers.add_parser("push", help="Push local BEP to server")
    push_parser.add_argument("ref", help="BEP number or slug")
    push_parser.add_argument("-y", "--yes", action="store_true", help="Skip confirmation")
    push_parser.add_argument("-n", "--note", help="Edit note for this version")
    push_parser.add_argument("--new-version", action="store_true", help="Create a new version")
    
    # open
    open_parser = subparsers.add_parser("open", help="Open BEP in browser")
    open_parser.add_argument("ref", help="BEP number or slug")
    
    args = parser.parse_args()
    
    if args.command is None:
        parser.print_help()
        return 0
    
    commands = {
        "pull": cmd_pull,
        "list": cmd_list,
        "new": cmd_new,
        "diff": cmd_diff,
        "push": cmd_push,
        "open": cmd_open,
    }
    
    return commands[args.command](args)


if __name__ == "__main__":
    sys.exit(main())
`;
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

  // Main index file (Claude.md)
  files.push({
    path: "Claude.md",
    content: generateClaudeMd(data),
  });

  // NEW-BEP instructions
  files.push({
    path: "NEW-BEP/INSTRUCTIONS.md",
    content: generateNewBepInstructions(nextNumber, apiBaseUrl, goodReferenceBeps),
  });

  // BEP skill for Claude Code
  files.push({
    path: "skills/bep.md",
    content: generateBepSkill(),
  });

  // BEP CLI script
  files.push({
    path: "bep",
    content: generateBepScript(apiBaseUrl),
  });

  // Individual BEP folders: BEP-<NUMBER>-<slug>/
  for (const bep of data.beps) {
    const bepFolder = formatBepFolderName(bep.number, bep.title);

    // Metadata JSON file
    files.push({
      path: `${bepFolder}/meta.json`,
      content: generateBepMetaJson(bep),
    });

    // Main content file (README.md)
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
