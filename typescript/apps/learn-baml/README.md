# Learn BAML

The canonical learning experience for BAML — a domain-specific language for building reliable AI features with typed LLM functions.

> [!IMPORTANT]  
> If you're an AI Agent or a human wanting to use AI to help write documentation, use the [CLAUDE.md](./CLAUDE.md) file. Read its entirety.

## Getting Started

```bash
cd typescript/apps/learn-baml
pnpm install
pnpm build
pnpm dev
```

> [!NOTE]  
> `pnpm build` runs `pnpm build:embeddings` which is important for local development. Run this so the AI agent always has up-to-date documentation. Embeddings are cached and saved to [./static/embeddings.json](./static/embeddings.json).

Open [http://localhost:3000](http://localhost:3000) to view the site.
The Bun API server listens on `3001` by default; set `BAML_API_PORT` to override.
Set `BAML_API_ORIGIN` if you need the dev proxy to point to a custom host.

## Serving Modes (`start` vs `serve`)

This distinction is important for LLM artifacts:

- `pnpm start` runs `docusaurus start` (dev server from source files). It does **not** serve post-build artifacts like `llms.txt` and `llms-full.txt`.
- `pnpm serve` runs `docusaurus serve` (serves the built `build/` output). Use this when validating crawler/agent artifacts.

To verify local `llms.txt` output:

```bash
pnpm build
pnpm serve
# then open http://localhost:3000/llms.txt
```

## Project Structure

```
src/
├── app/                        # Next.js App Router pages
│   ├── learn/                  # Router page ("How do you want to learn?")
│   ├── tour/                   # Interactive tour modules
│   ├── cookbook/               # Recipes (dynamic from content/)
│   ├── tutorials/              # Step-by-step guides
│   ├── how-to/                 # Task-oriented guides
│   ├── concepts/               # Explanations
│   └── reference/              # API/syntax reference
├── content/                    # MDX documentation files
│   ├── cookbook/               # Recipe MDX files
│   ├── tutorials/              # Tutorial MDX files
│   ├── how-to/                 # How-to guide MDX files
│   └── concepts/               # Concept MDX files
├── components/
│   ├── ui/                     # shadcn/ui components
│   ├── nav/                    # Navigation (TopNav, ThemeToggle)
│   ├── copy/                   # Copy buttons (CopyPageButton, CodeBlock)
│   ├── docs/                   # Doc layout components
│   ├── tour/                   # Tour components (TourRunner, TourProgress)
│   └── learn/                  # Learning path components
├── lib/
│   ├── mdx.ts                  # MDX content loading utilities
│   ├── tour-registry.ts        # Tour module metadata
│   ├── mock-runner.ts          # Mock BAML execution
│   └── utils.ts                # Utility functions (cn)
└── mdx-components.tsx          # MDX component overrides
```

## Adding Documentation

### Adding a New Recipe (Cookbook)

1. Create a new MDX file in `src/content/cookbook/`:

```mdx
---
title: My Recipe Title
description: A brief description for the index page
tags: [Tag1, Tag2]
order: 5
---

# My Recipe Title

Your content here with **markdown** and code blocks:

```baml
function Example(input: string) -> string {
  client "openai/gpt-4o"
  prompt #"{{ input }}"#
}
```
```

2. The recipe automatically appears on `/cookbook` and is accessible at `/cookbook/my-recipe-title`.

### MDX Frontmatter

| Field         | Required | Description                  |
| ------------- | -------- | ---------------------------- |
| `title`       | Yes      | Page title (h1 and metadata) |
| `description` | Yes      | Shown on index page          |
| `tags`        | No       | Array of tags for filtering  |
| `order`       | No       | Sort order (lower = first)   |

### Adding Other Content Types

The same pattern works for tutorials, how-to guides, and concepts:

- **Tutorials:** `src/content/tutorials/*.mdx` → `/tutorials/[slug]`
- **How-to:** `src/content/how-to/*.mdx` → `/how-to/[slug]`
- **Concepts:** `src/content/concepts/*.mdx` → `/concepts/[slug]`

## Documentation Framework (Diátaxis)

We follow the [Diátaxis framework](https://diataxis.fr/) for documentation:

|                 | **Learning-Oriented** | **Task-Oriented** |
| --------------- | --------------------- | ----------------- |
| **Practical**   | **Tutorials**         | **How-To Guides** |
| **Theoretical** | **Concepts**          | **Reference**     |

Additionally:
- **Tour:** Interactive, in-browser introduction (inspired by Go Tour, Gleam)
- **Cookbook:** Recipe-style code patterns with explanations (inspired by Rust Cookbook)

## LLM-Friendly Features

- **Copy Page Button:** Every page has a "Copy page" button that extracts content in LLM-friendly format
- **Copy Code Button:** Every code block has a copy button (appears on hover)
- Clean markdown output with preserved code blocks and source URLs

## Tech Stack

- **Framework:** Next.js 16 (App Router)
- **Styling:** Tailwind CSS v4 + shadcn/ui
- **Content:** MDX with `next-mdx-remote`
- **Icons:** Lucide React
- **Theme:** next-themes (light/dark mode)

## Development

```bash
# Start Docusaurus + Bun API server
pnpm dev

# Start only the Docusaurus dev server (source mode)
pnpm dev:site

# Start only the Bun API server (chat)
pnpm dev:api

# Build for production
pnpm build

# Serve built static output (includes llms.txt / llms-full.txt)
pnpm serve

# Run production Bun server
pnpm start:server
```

When documentation changes, regenerate embeddings before building or deploying:
`pnpm build:embeddings`

## Agent Surface Checks

After building, run:

```bash
pnpm check:agent-surface
```

This validates that key agent-facing artifacts and routes are present:
- `build/llms.txt`
- `build/llms-full.txt`
- seed routes such as `/agent-start-here`, `/tour/hello-baml`, and `/reference/baml-syntax`

## Related Documents

- [IMPLEMENTATION_PLAN.md](./IMPLEMENTATION_PLAN.md) — Detailed implementation plan and phases
- [release-plan/LEARN_BAML_SYSTEM_PLAN.md](../../release-plan/LEARN_BAML_SYSTEM_PLAN.md) — Information architecture plan
