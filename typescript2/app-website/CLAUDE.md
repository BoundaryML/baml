# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is a Next.js 15 marketing site for BAML (Boundary ML), a type-safe AI application development platform. The site uses TypeScript, React 19, TailwindCSS v4, and Biome for formatting/linting.

## Development Commands

```bash
# Development
npm run dev       # Start development server with Turbo (http://localhost:3000)

# Build & Production
npm run build     # Build production bundle
npm run start     # Start production server

# Code Quality
npm run lint      # Run Next.js linting
npm run typecheck # Run TypeScript type checking (tsc --noEmit)
npm run format    # Run Biome formatter
npm run format:fix # Fix formatting issues with Biome
```

## Architecture & Structure

### App Router Layout
- `/app` - Next.js 15 App Router structure
  - `/(home)` - Every page: the Boundary homepage (`/`), the blog (`/blog`), and the other pages
    (`/quickstart`, `/podcast`, `/who-are-we`, ...). One `layout.tsx` and
    `globals.css`, in the `@boundaryml/brand` package's BAML lane (dark only, Clover accent),
    fonts in `fonts/` (loaded by `fonts.ts`), the homepage's section styles in `styles/`. Pages
    other than the homepage and the blog wrap themselves in `PageShell`
    (`components/home/page-shell.tsx`) for the homepage's nav and footer. Only the homepage and
    the blog are linked from the nav and footer
  - `global-not-found.tsx` - The 404 page, which renders its own document with the same fonts,
    stylesheet, and shell
  - `/_lib/config.tsx` - Site configuration and constants
  - Layout uses server components by default

### Component Organization
- `/components` - Reusable React components
  - `/home` - The homepage's sections, nav and footer, which the blog shares.
    `analytics.ts` tags CTAs for PostHog autocapture; `track.tsx` sends the named
    `boundary_*` events
  - `/landing` - Landing page specific sections
  - `/magicui` - Animation and UI effect components
  - `/ui` - Base UI components (shadcn/ui based)
  - Theme-aware components using `next-themes`

### BAML Integration
- `/baml_src` - BAML schema definitions
  - `clients.baml` - AI provider configurations (OpenAI, Anthropic)
  - `generators.baml` - Code generation config
  - `resume.baml` - Example BAML schemas
- BAML generates TypeScript types automatically

### Styling
- TailwindCSS v4 with PostCSS
- CSS custom properties for theming
- Dark mode support via `next-themes`
- Utility classes via `cn()` helper from `/lib/utils`

## Key Development Patterns

### Component Conventions
- Use `.tsx` extension for React components
- File naming: kebab-case (enforced by Biome)
- Components use TypeScript strict mode
- Prefer named exports for components

### Code Style (Biome)
- Single quotes for strings
- Semicolons required
- Trailing commas enabled
- 2-space indentation
- No CommonJS imports (ESM only)
- No unused imports/variables errors

### Path Aliases
- `@/*` maps to project root
- Example: `import { cn } from '@/lib/utils'`

### Image Handling
- Remote images configured in `next.config.mjs`
- Supported domains include Unsplash, GitHub avatars, etc.

## AI Provider Configuration

The site showcases BAML features for AI development:
- Multiple LLM client configurations (GPT-4, Claude, etc.)
- Retry policies (Constant, Exponential)
- Provider strategies (Round-robin, Fallback)
- Environment variables required:
  - `OPENAI_API_KEY`
  - `ANTHROPIC_API_KEY`
