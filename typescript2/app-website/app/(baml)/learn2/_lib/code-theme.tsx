'use client';

import { createContext, useContext } from 'react';

/**
 * Shared theming for the inline code surfaces (the Shiki static `BamlCode`
 * panes and the Monaco `BamlEditor` / `LivePlayground`). One context drives all
 * three so a whole page can swap "IDE themes" from a single value.
 *
 * Add a preset here, register its Monaco theme in `baml-monarch.ts`, load its
 * Shiki theme in `highlighter.ts`, then flip `CODE_THEME` in the page (e.g.
 * Article.tsx) to try it. The default is `paper`, so pages that don't wrap in
 * `CodeThemeProvider` (the /learn2 deck) are unchanged.
 */
export type CodeThemeName = 'paper' | 'dark' | 'midnight';

export interface CodeThemePreset {
  /** Monaco theme id, registered in baml-monarch.ts */
  monaco: string;
  /** Shiki theme id, loaded in highlighter.ts */
  shiki: string;
  /** Shiki keyword recolor (light themes paint keywords red; we reserve red
   *  for diagnostics). Empty/undefined for dark themes. */
  shikiKeywordRemap?: Record<string, string>;
  /** whether this preset is a dark frame (drives the `data-code-theme` CSS) */
  dark: boolean;
}

export const CODE_THEMES: Record<CodeThemeName, CodeThemePreset> = {
  paper: {
    monaco: 'baml-paper',
    shiki: 'github-light',
    shikiKeywordRemap: { '#cf222e': '#0550ae' },
    dark: false,
  },
  dark: {
    monaco: 'baml-dark',
    shiki: 'github-dark',
    dark: true,
  },
  midnight: {
    monaco: 'baml-midnight',
    shiki: 'tokyo-night',
    dark: true,
  },
};

const CodeThemeContext = createContext<CodeThemeName>('paper');

export const CodeThemeProvider = CodeThemeContext.Provider;

/** The active preset (the resolved object, not just the name). */
export function useCodeTheme(): CodeThemePreset {
  return CODE_THEMES[useContext(CodeThemeContext)];
}
