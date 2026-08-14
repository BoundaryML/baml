# Theming Guide

This document describes the BEPs app design system and how to use it consistently.

## Design Principles

- **Single source of truth**: All colors live in `src/app/globals.css` as CSS custom properties.
- **Semantic tokens**: Use semantic names (`muted`, `foreground`, `code-bg`) instead of raw colors.
- **Accessibility**: Dark mode uses higher-contrast values for readability.

## Color Tokens

### Core Tokens

| Token | Light | Dark | Use |
|-------|------|------|-----|
| `background` | Page background | Slightly warm dark | Body, main surfaces |
| `foreground` | Primary text | High-contrast light | Body text |
| `card` | White | Elevated dark | Cards, popovers, overlays |
| `muted` | Light gray | Dark gray | Secondary surfaces, code bars |
| `muted-foreground` | Gray text | Brighter gray | Secondary text, captions |
| `primary` | Dark | Light | Buttons, links, accents |
| `border` | Light gray | Dark gray | Borders |
| `code-bg` | Light gray | Dark gray | Code block background |
| `code-fg` | Dark text | Light text | Code block text |
| `code-border` | Light gray | Dark gray | Code block borders |

### Usage in Components

```tsx
// Prefer semantic tokens
<div className="bg-card text-card-foreground border border-border" />
<code className="bg-code-bg text-code-fg border border-code-border" />

// Avoid hardcoded colors
<div className="bg-white dark:bg-gray-900" />  // Use bg-card instead
<div className="bg-gray-50 dark:bg-gray-800" />  // Use bg-muted instead
```

## Code Blocks

### Shiki (syntax-highlighted)

- **Config**: `src/lib/shiki-themes.ts`
- **Light**: `github-light`
- **Dark**: `github-dark-high-contrast` (higher contrast than default `github-dark`)

Shiki outputs dual-theme HTML. The `.dark` class on `<html>` switches to the dark variant. See `globals.css` for the CSS override.

### Plain / Fallback Code

Use semantic tokens for non-Shiki code (inline, ProseMirror, fallback pre):

```tsx
<code className="bg-code-bg text-code-fg border border-code-border" />
<pre className="bg-code-bg text-code-fg" />
```

## Extending the Theme

1. Add new tokens in `:root` and `.dark` in `globals.css`.
2. Register them in the `@theme inline` block.
3. Use them via Tailwind: `bg-<token>`, `text-<token>`, etc.

Example:

```css
:root {
  --custom: 200 50% 50%;
}
.dark {
  --custom: 200 50% 60%;
}
@theme inline {
  --color-custom: hsl(var(--custom));
}
```

## Dark Mode Toggle

Theme is stored in `localStorage` under `beps-theme` (see `src/lib/theme.ts`). Values: `light`, `dark`, `system`.
