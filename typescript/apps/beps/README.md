# BEP Feedback Application

A standalone web application for managing BAML Enhancement Proposals (BEPs) and their feedback lifecycle. Built with **Next.js 15** and **Convex** for real-time collaboration, it provides a centralized platform for proposal discussion, AI-assisted analysis, and knowledge consolidation.

## Table of Contents

- [Features](#features)
- [Architecture](#architecture)
- [Tech Stack](#tech-stack)
- [Getting Started](#getting-started)
- [Project Structure](#project-structure)
- [Data Model](#data-model)
- [Key Concepts](#key-concepts)
- [API Reference](#api-reference)
- [Export/Import](#exportimport)
- [AI Integration](#ai-integration)
- [Development](#development)

---

## Features

### BEP Management
- **Create and edit proposals** with versioning support
- **Multi-page BEPs** - each proposal can have additional wiki-like pages
- **Status tracking** - Draft → Proposed → Accepted → Implemented/Rejected/Superseded
- **Full version history** - every change creates a new version with diff viewing
- **Real-time updates** - all changes sync instantly via Convex subscriptions

### Commenting System
- **Threaded comments** - hierarchical discussions with parent-child relationships
- **Inline comments** - select text to create location-specific comments anchored to specific passages
- **Comment types** - Discussion, Concern (blocking), Question
- **Reactions** - emoji reactions (thumbs up/down, heart, thinking)
- **Resolution workflow** - mark comments as resolved/unresolved
- **Version-scoped** - comments are tied to specific BEP versions

### Decision & Issue Tracking
- **Record decisions** - capture key decisions with rationale and source comments
- **Track open issues** - manage action items with assignment and resolution
- **Cross-version navigation** - click linked comments to navigate to their version

### AI Assistant
- **Interactive Q&A** - ask questions about BEP content
- **Version comparison** - analyze what changed between versions
- **Quick actions** - summarize changes, list addressed concerns
- **Streaming responses** - real-time AI responses using Claude

### Import/Export
- **Export as ZIP** - download BEP with all content, comments, decisions, issues
- **Agent-friendly format** - inline comments embedded in markdown for AI readability
- **Import markdown** - upload edited files to create new versions

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              BROWSER                                         │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                     Next.js 15 App (App Router)                      │    │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐    │    │
│  │  │ BEP List │  │ BEP View │  │ Comments │  │ AI Assistant     │    │    │
│  │  │          │  │          │  │ (live)   │  │ (streaming)      │    │    │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────────────┘    │    │
│  │                                                                     │    │
│  │         Real-time subscriptions via Convex React hooks              │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                          WebSocket (automatic)
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           CONVEX BACKEND                                     │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                        Queries (real-time reads)                     │    │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │    │
│  │  │ beps.list    │  │ comments.    │  │ decisions.byBep          │  │    │
│  │  │ beps.get     │  │   byBep      │  │ issues.byBep             │  │    │
│  │  └──────────────┘  └──────────────┘  └──────────────────────────┘  │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                       Mutations (writes)                             │    │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐  │    │
│  │  │ beps.create  │  │ comments.add │  │ decisions.create         │  │    │
│  │  │ beps.update  │  │ comments.    │  │ issues.create            │  │    │
│  │  │              │  │   resolve    │  │ issues.resolve           │  │    │
│  │  └──────────────┘  └──────────────┘  └──────────────────────────┘  │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                        HTTP Actions (AI)                             │    │
│  │  ┌──────────────────────────────────────────────────────────────┐  │    │
│  │  │ /api/ai/stream-assistant - Streaming AI responses             │  │    │
│  │  └──────────────────────────────────────────────────────────────┘  │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                         CONVEX DATABASE                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌───────────┐    │
│  │ users    │  │ beps     │  │ comments │  │ decisions│  │ summaries │    │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘  └───────────┘    │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────────────────────────────┐  │
│  │ bepPages │  │openIssues│  │ bepVersions (content history)           │  │
│  └──────────┘  └──────────┘  └──────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Tech Stack

| Layer               | Technology                      | Purpose                                  |
| ------------------- | ------------------------------- | ---------------------------------------- |
| **Framework**       | Next.js 15 (App Router)         | Server components, streaming             |
| **Backend**         | Convex                          | Real-time database, serverless functions |
| **Language**        | TypeScript 5                    | Type safety                              |
| **Styling**         | Tailwind CSS 4                  | Utility-first styling                    |
| **UI Components**   | shadcn/ui (Radix primitives)    | Accessible, customizable components      |
| **Icons**           | Lucide React                    | Icon library                             |
| **Markdown**        | react-markdown + remark-gfm     | GitHub-flavored markdown rendering       |
| **AI**              | Anthropic SDK (Claude Sonnet 4) | AI-assisted analysis                     |
| **Package Manager** | Bun                             | Fast installs, native TypeScript         |

---

## Getting Started

### Prerequisites

- [Bun](https://bun.sh/) (recommended) or Node.js 18+
- A [Convex](https://convex.dev/) account
- An [Anthropic API key](https://console.anthropic.com/) (for AI features)

### Installation

1. **Clone the repository**
   ```bash
   git clone <repository-url>
   cd beps-app
   ```

2. **Install dependencies**
   ```bash
   bun install
   ```

3. **Set up Convex**
   ```bash
   bunx convex dev --once
   ```
   This creates the `convex/` folder and links to your Convex project.

4. **Configure environment variables**

   Copy `.env.local.example` to `.env.local` and fill in the values:
   ```bash
   cp .env.local.example .env.local
   ```

   Required variables:
   ```env
   # Convex (populated automatically by `convex dev`)
   CONVEX_DEPLOYMENT=your-deployment-name
   NEXT_PUBLIC_CONVEX_URL=https://your-deployment.convex.cloud

   # Anthropic API (for AI features)
   ANTHROPIC_API_KEY=sk-ant-...

   # Login Page Passkey
   LOGIN_PASSKEY=password
   ```

5. **Start the development server**
   ```bash
   bun run dev
   ```

   This runs both Next.js and Convex dev servers in parallel.

6. **Open the app**

   Navigate to [http://localhost:3000](http://localhost:3000)

### First-time Setup

1. Visit the app - you'll be prompted to enter your name
2. Your name-based identity is stored locally and synced with Convex
3. Start creating BEPs!

---

## Project Structure

```
beps-app/
├── convex/                           # Backend (Convex functions)
│   ├── schema.ts                     # Database schema definition
│   ├── users.ts                      # User queries/mutations
│   ├── beps.ts                       # BEP CRUD + versioning
│   ├── comments.ts                   # Comment operations
│   ├── decisions.ts                  # Decision tracking
│   ├── issues.ts                     # Issue management
│   ├── export.ts                     # Export query
│   ├── http.ts                       # HTTP endpoints (AI streaming)
│   ├── migrations.ts                 # Data migrations
│   └── lib/
│       └── prompts.ts                # AI prompt templates
│
├── src/
│   ├── app/                          # Next.js App Router pages
│   │   ├── layout.tsx                # Root layout with providers
│   │   ├── page.tsx                  # Home (BEP list)
│   │   ├── login/page.tsx            # Login page
│   │   ├── api/agent/beps/route.ts   # Public read-only BEP context API
│   │   └── beps/
│   │       ├── new/page.tsx          # Create BEP
│   │       └── [number]/
│   │           ├── page.tsx          # View BEP
│   │           └── edit/page.tsx     # Edit BEP
│   │
│   ├── components/
│   │   ├── ui/                       # shadcn/ui components
│   │   ├── providers/                # Context providers
│   │   ├── bep/                      # BEP-specific components
│   │   ├── comments/                 # Comment system
│   │   ├── decisions/                # Decision tracking
│   │   ├── issues/                   # Issue management
│   │   └── ai-assistant/             # AI assistant panel
│   │
│   ├── hooks/
│   │   └── use-text-selection.ts     # Text selection for inline comments
│   │
│   └── lib/
│       ├── utils.ts                  # Utility functions
│       ├── markdown.tsx              # Markdown styling
│       ├── export-utils.ts           # ZIP export formatting
│       └── import-utils.ts           # ZIP import parsing
│
├── package.json
├── tsconfig.json
├── next.config.ts
├── components.json                   # shadcn/ui config
└── .env.local.example
```

---

## Data Model

### Core Tables

#### Users
Simple name-based authentication for lightweight collaboration.

| Field       | Type                              | Description        |
| ----------- | --------------------------------- | ------------------ |
| `name`      | string                            | Display name       |
| `avatarUrl` | string?                           | Optional avatar    |
| `role`      | "admin" \| "shepherd" \| "member" | User role          |
| `createdAt` | number                            | Creation timestamp |

#### BEPs (Enhancement Proposals)
The core entity representing a proposal.

| Field       | Type                                                                               | Description                |
| ----------- | ---------------------------------------------------------------------------------- | -------------------------- |
| `number`    | number                                                                             | BEP number (e.g., 1, 2, 3) |
| `title`     | string                                                                             | Proposal title             |
| `status`    | "draft" \| "proposed" \| "accepted" \| "implemented" \| "rejected" \| "superseded" | Current status             |
| `content`   | string                                                                             | Main markdown content      |
| `shepherds` | Id<"users">[]                                                                      | Assigned shepherds         |
| `createdAt` | number                                                                             | Creation timestamp         |
| `updatedAt` | number                                                                             | Last update timestamp      |

#### BEP Versions
Tracks content history - every edit creates a new version.

| Field           | Type           | Description                 |
| --------------- | -------------- | --------------------------- |
| `bepId`         | Id<"beps">     | Parent BEP                  |
| `version`       | number         | Version number (1, 2, 3...) |
| `title`         | string         | Title at this version       |
| `content`       | string         | Content snapshot            |
| `pagesSnapshot` | PageSnapshot[] | All pages at this version   |
| `editedBy`      | Id<"users">    | Who made this edit          |
| `editNote`      | string?        | Optional change description |
| `createdAt`     | number         | When version was created    |

#### BEP Pages
Additional wiki-like pages within a BEP.

| Field     | Type       | Description      |
| --------- | ---------- | ---------------- |
| `bepId`   | Id<"beps"> | Parent BEP       |
| `slug`    | string     | URL-friendly ID  |
| `title`   | string     | Page title       |
| `content` | string     | Markdown content |
| `order`   | number     | Sort order       |

#### Comments
Threaded discussions with inline commenting support.

| Field       | Type                                    | Description                |
| ----------- | --------------------------------------- | -------------------------- |
| `bepId`     | Id<"beps">                              | Parent BEP                 |
| `versionId` | Id<"bepVersions">                       | Version this comment is on |
| `pageId`    | Id<"bepPages">?                         | Page (null = main content) |
| `authorId`  | Id<"users">                             | Comment author             |
| `parentId`  | Id<"comments">?                         | Parent for threading       |
| `type`      | "discussion" \| "concern" \| "question" | Comment type               |
| `content`   | string                                  | Markdown content           |
| `anchor`    | Anchor?                                 | Inline comment position    |
| `reactions` | Reactions?                              | Emoji reactions            |
| `resolved`  | boolean                                 | Resolution status          |

**Anchor structure** (for inline comments):
```typescript
{
  selectedText: string;   // The exact text commented on
  lineNumber: number;     // Line number in content
  lineContent: string;    // Full line content for matching
}
```

#### Decisions
Records of key decisions made during discussion.

| Field              | Type             | Description          |
| ------------------ | ---------------- | -------------------- |
| `bepId`            | Id<"beps">       | Parent BEP           |
| `title`            | string           | Decision title       |
| `description`      | string           | What was decided     |
| `rationale`        | string?          | Why this was decided |
| `sourceCommentIds` | Id<"comments">[] | Source comments      |
| `participants`     | Id<"users">[]    | People involved      |
| `decidedAt`        | number           | When decided         |

#### Open Issues
Action items and problems to be resolved.

| Field               | Type              | Description         |
| ------------------- | ----------------- | ------------------- |
| `bepId`             | Id<"beps">        | Parent BEP          |
| `title`             | string            | Issue title         |
| `description`       | string?           | Details             |
| `raisedBy`          | Id<"users">       | Who raised it       |
| `assignedTo`        | Id<"users">?      | Assignee            |
| `relatedCommentIds` | Id<"comments">[]? | Related comments    |
| `resolved`          | boolean           | Resolution status   |
| `resolution`        | string?           | How it was resolved |

---

## Key Concepts

### Version-Scoped Comments

Comments are tied to specific BEP versions. When viewing a historical version:
- You see only comments from that version
- The comment form is disabled (read-only mode)
- A banner indicates you're viewing history

This ensures feedback is always in context and prevents orphaned comments when content changes.

### Inline Comments

Select any text in the BEP content to attach a comment directly to that passage:

1. Select text in the content area
2. Click the floating "Add Comment" button
3. Choose comment type and write your feedback
4. The comment appears as a marker in the right margin

Inline comments are anchored by:
- The selected text
- Line number
- Line content (for matching)

### Real-time Collaboration

All data updates are live via Convex WebSocket subscriptions:
- Open a BEP in two browser windows
- Add a comment in one - it appears instantly in the other
- No refresh needed

### Issues vs Decisions

- **Issues**: Open problems that need resolution (action items)
- **Decisions**: Recorded outcomes from discussions (historical record)

Both can be created from comments to maintain traceability.

---

## API Reference

### Convex Queries

| Query                               | Description                        |
| ----------------------------------- | ---------------------------------- |
| `beps.list(status?, limit?)`        | List BEPs with optional filtering  |
| `beps.getByNumber(number)`          | Get BEP with all related data      |
| `beps.getNextNumber()`              | Get next available BEP number      |
| `comments.byBep(bepId)`             | All comments for a BEP             |
| `comments.byBepPage(...)`           | Comments for specific page/version |
| `decisions.byBep(bepId)`            | All decisions for a BEP            |
| `issues.byBep(bepId)`               | All issues for a BEP               |
| `export.getFullBepForExport(bepId)` | Complete BEP data for export       |

### Convex Mutations

| Mutation                  | Description                      |
| ------------------------- | -------------------------------- |
| `users.getOrCreate(name)` | Get or create user by name       |
| `beps.create(...)`        | Create new BEP                   |
| `beps.update(...)`        | Update BEP (creates new version) |
| `beps.updateStatus(...)`  | Change BEP status                |
| `beps.importVersion(...)` | Import content as new version    |
| `comments.create(...)`    | Add comment                      |
| `comments.resolve(...)`   | Mark comment resolved            |
| `decisions.create(...)`   | Record decision                  |
| `issues.create(...)`      | Create issue                     |
| `issues.resolve(...)`     | Resolve issue                    |

### HTTP Endpoints

| Endpoint                   | Method | Description                                   |
| -------------------------- | ------ | --------------------------------------------- |
| `/api/ai/stream-assistant` | POST   | Stream AI responses for Q&A                   |
| `/api/agent/beps`          | GET    | Public read-only BEP listing/fetch for agents |

### Public Agent Endpoint

`GET /api/agent/beps`

- Without query params: lists all BEPs.
- With `name=<bep-name-or-id>` (also accepts `query` or `q`): fuzzy-matches and returns a BEP bundle.
- Defaults to including all versions/history.
- Add `omitOtherVersions=true` to omit historical versions from the returned bundle.
- Add `format=markdown` to get raw markdown output instead of JSON.

#### Query Parameters

| Param | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | string | No | Fuzzy BEP matcher (preferred key). |
| `query` | string | No | Alias for `name`. |
| `q` | string | No | Alias for `name`. |
| `omitOtherVersions` | boolean-ish | No | Truthy values (`1`, `true`, `yes`, `y`, `on`) omit `history/*` files. |
| `format` | string | No | `json` (default) or `markdown`. |

#### Success Responses

`200` list mode (`GET /api/agent/beps` with no query):

```json
{
  "mode": "list",
  "total": 2,
  "beps": [
    {
      "id": "BEP-001",
      "number": 1,
      "title": "Structured Error Payloads",
      "status": "accepted",
      "updatedAt": "2026-02-18T20:13:34.000Z"
    }
  ],
  "usage": {
    "list": "/api/agent/beps",
    "fetch": "/api/agent/beps?name=<bep-name-or-id>",
    "omitOtherVersions": "/api/agent/beps?name=<bep-name-or-id>&omitOtherVersions=true"
  }
}
```

`200` matched BEP JSON mode (`GET /api/agent/beps?name=<...>`):

```json
{
  "mode": "bep",
  "query": "structured error payloads",
  "matched": {
    "id": "BEP-001",
    "number": 1,
    "title": "Structured Error Payloads",
    "status": "accepted",
    "score": 1.732
  },
  "currentVersion": 5,
  "omitOtherVersions": false,
  "markdown": "<!-- FILE: README.md -->\n# BEP-001 ...",
  "files": [
    {
      "path": "README.md",
      "content": "# BEP-001 ..."
    }
  ]
}
```

Schema for `mode: "bep"` JSON responses:

| Field | Type | Notes |
| --- | --- | --- |
| `mode` | string | Always `"bep"` in this response shape. |
| `query` | string | Normalized user query used for fuzzy matching. |
| `matched` | object | Matched BEP metadata: `id`, `number`, `title`, `status`, `score`. |
| `currentVersion` | number | Current BEP version number. |
| `omitOtherVersions` | boolean | Echoes resolved filter flag from query params. |
| `markdown` | string | Flattened markdown bundle content (all selected `.md` files). |
| `files` | array | Per-file markdown entries: `{ path, content }`. |

`200` matched BEP markdown mode (`GET /api/agent/beps?name=<...>&format=markdown`):

```markdown
<!-- FILE: README.md -->
# BEP-001 Structured Error Payloads
...
```

#### Error Responses

`404` (no fuzzy match found):

```json
{
  "error": "Could not find a BEP that matches \"<query>\".",
  "suggestions": [
    { "id": "BEP-003", "title": "..." },
    { "id": "BEP-010", "title": "..." }
  ]
}
```

`500` (server misconfiguration, missing Convex URL):

```json
{
  "error": "Missing NEXT_PUBLIC_CONVEX_URL environment variable."
}
```

`502` (upstream Convex failure, or unrecognized export payload shape):

```json
{
  "error": "Failed to fetch BEP list.",
  "detail": "<error message>"
}
```

```json
{
  "error": "Failed to fetch BEP export data.",
  "detail": "<error message>"
}
```

```json
{
  "error": "Invalid BEP export payload shape."
}
```

#### CORS / Preflight

- `OPTIONS /api/agent/beps` is supported for browser preflight and returns `204`.
- CORS headers: `Access-Control-Allow-Origin: *`, `Access-Control-Allow-Methods: GET, OPTIONS`, `Access-Control-Allow-Headers: Content-Type`.

You should install the `beps` skill through our [skills](https://github.com/BoundaryML/skills) repository.

---

## Export/Import

### Export Format

When you export a BEP, you get a ZIP file with this structure:

```
BEP-001/
├── README.md                 # Main content with inline comments embedded
├── pages/
│   ├── background.md         # Additional pages with comments
│   └── tooling.md
├── AGENT_CONTEXT.md          # AI-friendly summary
├── metadata.json             # Machine-readable metadata
├── discussion/
│   ├── issues.md             # Open and resolved issues
│   └── decisions.md          # Recorded decisions
└── history/
    ├── versions.md           # Version history
    └── summaries.md          # AI-generated summaries
```

### Comment Embedding

Comments are embedded directly in the markdown content:

**Inline comments** appear next to the referenced text:
```markdown
**catch as an operator on blocks**.

<!-- INLINE_COMMENT
version: 3
line: 4
selected_text: "catch as an operator on blocks"
author: Dave
date: 2025-01-20
type: suggestion
status: open
-->
> Should we add an example showing operator precedence?
<!-- /INLINE_COMMENT -->
```

**General comments** appear at the end:
```markdown
---

<!-- GENERAL_COMMENTS -->

## Comments

### Concern by Alice (v2, 2025-01-15) [OUTDATED]
> This might confuse newcomers

<!-- /GENERAL_COMMENTS -->
```

### Import Workflow

1. Export a BEP
2. Edit the markdown files externally (IDE, AI agent, etc.)
3. Import the files back
4. A new version is created with clean content (comments stripped)
5. The new version starts with zero comments - users add fresh feedback

---

## AI Integration

The AI Assistant uses Claude (Sonnet 4) to help analyze and understand BEPs.

### Features

- **Version comparison**: "What changed between v2 and v5?"
- **Summarize changes**: Quick summary of what's different
- **List addressed concerns**: Find which concerns were resolved
- **Custom questions**: Ask anything about the BEP content

### How It Works

1. Open the AI Assistant panel on any BEP
2. Select versions to compare (optional)
3. Ask a question or use a quick action
4. Watch the streaming response

The AI has full context including:
- Content from both versions (if comparing)
- All comments from those versions
- Decisions and issues
- Version metadata

### Configuration

Set your Anthropic API key in the Convex dashboard:
1. Go to your Convex project
2. Navigate to Settings → Environment Variables
3. Add `ANTHROPIC_API_KEY` with your key

---

## Development

### Available Scripts

```bash
# Development (runs Next.js + Convex in parallel)
bun run dev

# Run only Next.js
bun run dev:next

# Run only Convex
bun run dev:convex

# Build for production
bun run build

# Start production server
bun run start

# Run linting
bun run lint
```

### Adding shadcn/ui Components

```bash
bunx --bun shadcn@latest add <component-name>
```

Components are added to `src/components/ui/`.

### Database Migrations

When you need to migrate data, create a migration in `convex/migrations.ts` and run it via a Convex action.

### Deploying

1. **Deploy Convex**
   ```bash
   bunx convex deploy
   ```

2. **Deploy Next.js** (e.g., to Vercel)
   - Connect your repository to Vercel
   - Set environment variables:
     - `NEXT_PUBLIC_CONVEX_URL` - Your production Convex URL
   - Deploy

3. **Set Convex environment variables**
   - In Convex dashboard, add `ANTHROPIC_API_KEY` to production

---

## Design Principles

1. **Data-first**: BEPs, comments, decisions are structured data, not files
2. **Real-time by default**: All data updates are live via Convex subscriptions
3. **AI as assistant, human as authority**: AI suggests, humans approve
4. **Version everything**: BEP content changes are tracked as versions
5. **Export-friendly**: Data can always be exported to markdown/git if needed

---

## License

[Add your license here]

---

## Contributing

[Add contribution guidelines here]
