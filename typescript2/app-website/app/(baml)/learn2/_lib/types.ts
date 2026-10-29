import type { ReactNode } from 'react';

export type Severity = 'error' | 'warning' | 'info';

export type CodeLang =
  | 'baml'
  | 'python'
  | 'typescript'
  | 'bash'
  | 'go'
  | 'rust'
  | 'json'
  | 'yaml';

/** An Error-Lens style inline diagnostic, rendered at the end of a line. */
export interface Diagnostic {
  /** 1-based line number. */
  line: number;
  severity: Severity;
  message: string;
}

/** A margin annotation that points at a specific line (the "arrows + notes" idea). */
export interface CodeNote {
  /** 1-based line number. */
  line: number;
  text: string;
}

export interface BamlCodeProps {
  code: string;
  lang?: CodeLang;
  /** Optional filename shown in the editor chrome (e.g. "sentiment.baml"). */
  filename?: string;
  diagnostics?: Diagnostic[];
  notes?: CodeNote[];
  /** Lines to softly emphasise (1-based). */
  highlightLines?: number[];
  /** Number the first rendered line as this value (default 1). */
  startLine?: number;
  /** Hide the gutter line numbers. */
  noLineNumbers?: boolean;
  /**
   * Soft-wrap long lines within the column instead of scrolling horizontally.
   * Use in narrow pair columns where a long inline diagnostic would otherwise
   * force a horizontal scrollbar (matches the Monaco `wordWrap` side).
   */
  wrap?: boolean;
}

/** Metadata for a single slide — serialisable, passed to the client Deck. */
export interface SlideMeta {
  id: string;
  section: string;
  title: string;
}

/** A slide: serialisable meta + its (server-rendered) React node. */
export interface Slide extends SlideMeta {
  node: ReactNode;
}
