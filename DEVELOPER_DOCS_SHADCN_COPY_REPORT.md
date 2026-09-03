# Developer Documentation shadcn Copy Report

## Result

For the current documentation-shell implementation:

- **49.7% is directly copied and mechanically adapted from the pinned shadcn v4 reference.**
- **50.3% is not copied from shadcn.** It is Boundary-specific content, interaction code, compatibility work, or locally designed implementation.

This is an implementation-provenance measurement, not a visual-similarity score. Code that merely recreates a shadcn-like result does **not** count as copied.

The stricter near-verbatim-line measurement is **28.5% copied / 71.5% not copied**. It counts only long, meaningful, non-import lines that still match the mapped reference files after whitespace, quote-style, and trailing-semicolon normalization. It intentionally misses copied lines that required JSX, dependency, icon, product-name, or Tailwind compatibility edits.

## Scope and reference

- Local implementation: `typescript2/app-developer-docs`
- Reference: `/Users/vbv/repos/reference/shadcn-ui/apps/v4`
- Pinned reference commit: `63c1308d112b6b1205d86244a156cca1abef5087`
- Snapshot date: 2026-09-02
- License notice: `typescript2/app-developer-docs/THIRD_PARTY_NOTICES.md`

The primary percentage covers the visible shell: global styles and Typeset rules, root layout, homepage composition, header, footer, primary and mobile navigation, documentation sidebar/article/TOC layout, page actions, theme control, search dialog, brand mark, reusable documentation card, and custom 404. It excludes route content, database and generation code, validators, scripts, tests, and configuration because those are not shadcn shell code.

## What “copied” means

A block counts as directly copied and adapted only when all of these are true:

1. A concrete source block exists in the pinned reference.
2. The local block was transferred from that source block.
3. Its control flow, JSX hierarchy, CSS-token set, or Tailwind utility sequence remains recognizably the same after mechanical changes.

Mechanical changes include replacing shadcn identity with BAML/Boundary identity, swapping icons, changing imports to locally compatible components, removing product-specific controls, and adjusting syntax for the selected Next.js/Tailwind versions.

The following do **not** count as copied:

- Similar behavior written independently.
- Generic React or Next.js boilerplate.
- A layout that only looks shadcn-like.
- Boundary-specific navigation data, product copy, routes, brand art, search indexing, or 404 recovery content.
- Code taken from any source other than the pinned shadcn reference.

## Block-by-block audit

The numerator is a conservative line-equivalent count from a block-by-block provenance review. It is not inferred from appearance.

| Local file | Nonblank lines | Directly copied/adapted line-equivalents | Copied share | Principal reference |
|---|---:|---:|---:|---|
| `app/globals.css` | 352 | 222 | 63% | `app/globals.css`, `app/(app)/(typeset)/typeset.css` |
| `app/layout.tsx` | 47 | 35 | 74% | `app/layout.tsx`, `app/(app)/layout.tsx` |
| `app/page.tsx` | 124 | 70 | 56% | `app/(app)/(root)/page.tsx`, root card-rail layout, `components/page-header.tsx` |
| `components/main-nav.tsx` | 28 | 24 | 86% | `components/main-nav.tsx` |
| `components/site-header.tsx` | 44 | 35 | 80% | `components/site-header.tsx` |
| `components/site-footer.tsx` | 27 | 20 | 74% | `components/site-footer.tsx` |
| `components/page-header.tsx` | 32 | 28 | 88% | `components/page-header.tsx` |
| `components/mobile-nav.tsx` | 55 | 22 | 40% | `components/mobile-nav.tsx` |
| `components/docs-sidebar.tsx` | 256 | 78 | 30% | `components/docs-sidebar.tsx` |
| `components/docs-shell.tsx` | 152 | 91 | 60% | Docs layout/page, TOC, and right-rail CTA components |
| `components/docs-page-actions.tsx` | 120 | 45 | 38% | Docs page navigation and `components/docs-copy-page.tsx` |
| `components/theme-provider.tsx` | 14 | 14 | 100% | `components/theme-provider.tsx` |
| `components/theme-toggle.tsx` | 38 | 33 | 87% | `components/mode-switcher.tsx` |
| `components/search-menu.tsx` | 121 | 27 | 22% | `components/command-menu.tsx` |
| `components/brand-mark.tsx` | 16 | 0 | 0% | Boundary-original |
| `components/docs-card.tsx` | 24 | 0 | 0% | Boundary-original |
| `app/not-found.tsx` | 48 | 0 | 0% | Boundary-original contextual 404 |
| **Shell total** | **1,498** | **744** | **49.7%** | **50.3% not copied** |

The largest non-copied portions are the recursive collapsible tree, persisted disclosure state, static search implementation, BAML/Boundary information architecture, nested breadcrumbs, labeled bottom navigation, route data, brand mark, contextual 404, and local callout/table styling. The copied portions include the design tokens, Geist typography setup, core Typeset rules, responsive containers, root layout, header/footer structure, homepage hero/card rail, primary/mobile navigation frame, docs grid, scroll-preserving sidebar frame, 640px article column, 288px TOC rail, top page actions, right-rail CTA pattern, and theme patterns.

## Strict near-verbatim-line measurement

For an independently reproducible lower bound, the shell was compared only with its mapped reference files as follows:

- Ignore blank and comment-only lines.
- Ignore lines shorter than 20 normalized characters so braces and trivial syntax do not inflate the result.
- Ignore import and export lines so generic module boilerplate does not count as copying.
- Normalize surrounding whitespace, repeated internal whitespace, single versus double quotes, and a trailing semicolon.
- Require the complete normalized line to exist in the mapped reference source.

That comparison found **166 matching lines out of 583 eligible lines**, or **28.5% copied / 71.5% not copied**. This metric is strict enough to reject “inspired by” code, but it undercounts direct copy-and-adapt work whenever a necessary local edit changes the line.

## Whole-application context

Across all non-test TypeScript, TSX, and CSS under the portal's `app`, `components`, and `lib` directories, there are 4,427 nonblank lines in this snapshot. Reusing the shell audit numerator gives a conservative whole-application result of **16.8% copied/adapted / 83.2% not copied**. That number is expected to be much lower because database access, release generation, route contracts, validation, and BAML content are product-specific and must not come from shadcn.

## Interpretation

The current visible shell is almost evenly split: 49.7% is directly traceable to transferred shadcn reference blocks and 50.3% is local implementation. The copied share decreased because the requested recursive tree, open-state behavior, deep hierarchy, breadcrumbs, tables, callouts, and labeled page navigation have no directly copied counterpart in shadcn's flat documentation rail. The visual and interaction review remains separate from this provenance percentage: the shell keeps shadcn's measured geometry and visual language without mislabeling new hierarchy code as copied.

This report must be recalculated if the shell files change before final sign-off. It is a requested audit artifact, not an upstream synchronization mechanism, patch queue, or claim that the reference remains authoritative.
