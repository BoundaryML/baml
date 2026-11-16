/**
 * The brand's code palette for the workbench's Monaco theme (baml-monarch.ts).
 * Monaco needs literal hex values, so the site's tokens are spelled out here
 * with the token each one copies.
 *
 * The syntax colours follow the homepage's code (components/home/ui.tsx):
 * Clover keywords, white types, periwinkle strings, and muted comments.
 */
export const CODE = {
  /** --body: plain identifiers and punctuation. */
  base: '#BDBDC4',
  /** --muted: comments and active line numbers. */
  comment: '#828280',
  /** --color-destructive in globals.css: errors. */
  error: '#E5484D',
  /** --faint: line numbers. */
  faint: '#3F3F3C',
  /** --ink: function names. */
  fn: '#F4F4F2',
  /** Clover (--accent): keywords, the cursor, and the selection tint. */
  keyword: '#CAAAF2',
  /** The current line, one step above the panel. */
  lineHi: '#1A1A1A',
  /** Clover 100 (--color-clover-100): numbers and other literals. */
  literal: '#E4D1FF',
  /** --raised: the editor panel. */
  panel: '#121212',
  /** --ink-2: properties. */
  prop: '#D6D6D2',
  /** --rule: hairlines inside the editor. */
  rule: '#262626',
  /** Periwinkle (--color-periwinkle in globals.css): strings. */
  string: '#A3B2EE',
  /** White (--color-white): types. */
  type: '#FFFFFF',
  /** Clover 300 (--color-clover-300): warnings. */
  warning: '#B484EA',
} as const;

/** The editors' font: the site's mono face (font-mono, JetBrains Mono). */
export const MONO_FONT = 'var(--font-jetbrains-mono), ui-monospace, monospace';
