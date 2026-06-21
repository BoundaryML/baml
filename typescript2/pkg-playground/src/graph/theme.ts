/**
 * Theme detection for the graph.
 *
 * The graph renders in two surfaces:
 *  - The VS Code webview, where the editor injects `--vscode-*` CSS vars and
 *    sets `document.body[data-vscode-theme-kind]` (e.g. "vscode-dark").
 *  - The browser playground, where `playground.css` drives `--background` and
 *    it falls back to a dark surface (#1f1f1f) when no editor vars exist.
 *
 * Detection order (most → least authoritative):
 *  1. `data-vscode-theme-kind` on <body> — VS Code tells us exactly.
 *  2. Resolved luminance of the panel `--background` — works in any host
 *     because the var chain is painted onto a probe element and read back as
 *     a concrete rgb() value.
 *  3. `prefers-color-scheme` — last-resort OS hint.
 */
import { createContext, useContext, useSyncExternalStore } from 'react';

export type GraphTheme = 'light' | 'dark';

/** Parse "rgb(r, g, b)" / "rgba(r, g, b, a)" into [r,g,b,a]. */
function parseRgb(value: string): [number, number, number, number] | null {
  const m = value.match(
    /rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:[\s,/]+([\d.]+))?/i,
  );
  if (!m) return null;
  return [
    Number(m[1]),
    Number(m[2]),
    Number(m[3]),
    m[4] === undefined ? 1 : Number(m[4]),
  ];
}

/** Perceptual luminance (sRGB, 0..1). */
function luminance([r, g, b]: [number, number, number]): number {
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
}

/**
 * Resolve the current theme by inspecting the live DOM. Cheap enough to call
 * from non-React code paths (edge color computation, convert) which run only
 * when the graph changes.
 */
export function resolveGraphTheme(): GraphTheme {
  if (typeof document === 'undefined' || !document.body) return 'light';

  // 1. VS Code is explicit about its theme kind.
  const kind = document.body.dataset?.vscodeThemeKind;
  if (kind) return kind.includes('light') ? 'light' : 'dark';

  // 2. Resolve the painted panel background. A throwaway probe forces the
  //    `var(--background, …)` chain to collapse to a concrete rgb() value,
  //    which `getComputedStyle` on a custom property would not do.
  try {
    const probe = document.createElement('div');
    // Chain through the same fallbacks playground.css uses, ending at the
    // dark #1f1f1f default — so a host that never defines --background still
    // resolves to a concrete color instead of transparent.
    probe.style.cssText =
      'position:absolute;width:0;height:0;visibility:hidden;background:var(--background, var(--vscode-editor-background, #1f1f1f))';
    document.body.appendChild(probe);
    const bg = getComputedStyle(probe).backgroundColor;
    probe.remove();
    const rgb = parseRgb(bg);
    if (rgb && rgb[3] !== 0) {
      return luminance([rgb[0], rgb[1], rgb[2]]) < 0.5 ? 'dark' : 'light';
    }
  } catch {
    /* DOM not ready / sandboxed — fall through */
  }

  // 3. OS preference.
  if (
    typeof window !== 'undefined' &&
    window.matchMedia?.('(prefers-color-scheme: dark)').matches
  ) {
    return 'dark';
  }
  return 'light';
}

// ── Reactive store backing useGraphTheme ──────────────────────────────────
// getSnapshot must be cheap & stable, so we cache the resolved value and only
// recompute when a subscribed signal (theme attr, OS preference) fires.
let cached: GraphTheme | null = null;

function recompute(): void {
  cached = resolveGraphTheme();
}

function getSnapshot(): GraphTheme {
  if (cached === null) recompute();
  return cached as GraphTheme;
}

function subscribe(onChange: () => void): () => void {
  const notify = () => {
    const prev = cached;
    recompute();
    if (cached !== prev) onChange();
  };

  const cleanups: Array<() => void> = [];

  if (typeof document !== 'undefined') {
    const observer = new MutationObserver(notify);
    observer.observe(document.body, {
      attributes: true,
      attributeFilter: ['data-vscode-theme-kind', 'class', 'style'],
    });
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class', 'style'],
    });
    cleanups.push(() => observer.disconnect());
  }

  if (typeof window !== 'undefined' && window.matchMedia) {
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    mq.addEventListener('change', notify);
    cleanups.push(() => mq.removeEventListener('change', notify));
  }

  return () => cleanups.forEach((fn) => fn());
}

/** Live theme that re-renders the consumer when the surface theme changes. */
export function useGraphTheme(): GraphTheme {
  return useSyncExternalStore(subscribe, getSnapshot, () => 'light');
}

/**
 * Resolved theme shared with all node/edge components via a single provider,
 * so we don't attach a DOM observer per node.
 */
export const GraphThemeContext = createContext<GraphTheme>('light');

export function useGraphThemeContext(): GraphTheme {
  return useContext(GraphThemeContext);
}
