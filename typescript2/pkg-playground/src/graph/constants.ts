/**
 * Shared state colors & visual tokens for the graph.
 *
 * The palette targets a modern, professional dark-mode aesthetic:
 *  - Subtle, low-contrast borders (the node sits on the canvas, not above it).
 *  - State is encoded via the accent (left rail / icon chip), not heavy borders.
 *  - Backgrounds use a soft dual-stop gradient for a refined depth effect.
 */
export interface NodeStateStyle {
  /** Tinted hairline border; the node frame. */
  border: string;
  /** Top-of-gradient background — the brighter face of the card. */
  bgTop: string;
  /** Bottom-of-gradient background — the darker face. */
  bgBottom: string;
  /** Solid accent color used for icon chips, badges, the left rail. */
  accent: string;
  /** Soft, transparent halo for the running/active glow. */
  glow: string;
  /** Foreground used for label text. */
  text: string;
  /** Muted foreground used for sublabels (e.g. LLM client). */
  textMuted: string;
}

export const stateColors: Record<string, NodeStateStyle> = {
  'not-started': {
    border: 'rgba(255,255,255,0.08)',
    bgTop: 'rgba(46,46,52,0.85)',
    bgBottom: 'rgba(32,32,38,0.85)',
    accent: '#3f3f46',
    glow: 'rgba(0,0,0,0)',
    text: '#e5e7eb',
    textMuted: '#9ca3af',
  },
  'running': {
    border: 'rgba(59,130,246,0.45)',
    bgTop: 'rgba(30,58,95,0.85)',
    bgBottom: 'rgba(20,38,68,0.85)',
    accent: '#3b82f6',
    glow: 'rgba(59,130,246,0.35)',
    text: '#dbeafe',
    textMuted: '#93c5fd',
  },
  'success': {
    border: 'rgba(16,185,129,0.40)',
    bgTop: 'rgba(20,55,46,0.85)',
    bgBottom: 'rgba(14,40,32,0.85)',
    accent: '#10b981',
    glow: 'rgba(16,185,129,0.25)',
    text: '#d1fae5',
    textMuted: '#86efac',
  },
  'error': {
    border: 'rgba(244,63,94,0.45)',
    bgTop: 'rgba(64,21,30,0.85)',
    bgBottom: 'rgba(46,16,22,0.85)',
    accent: '#f43f5e',
    glow: 'rgba(244,63,94,0.30)',
    text: '#fecdd3',
    textMuted: '#fda4af',
  },
  'cancelled': {
    border: 'rgba(234,179,8,0.40)',
    bgTop: 'rgba(58,45,18,0.85)',
    bgBottom: 'rgba(42,32,12,0.85)',
    accent: '#eab308',
    glow: 'rgba(234,179,8,0.22)',
    text: '#fef9c3',
    textMuted: '#fde047',
  },
  'pending': {
    border: 'rgba(245,158,11,0.40)',
    bgTop: 'rgba(58,40,18,0.85)',
    bgBottom: 'rgba(40,28,14,0.85)',
    accent: '#f59e0b',
    glow: 'rgba(245,158,11,0.25)',
    text: '#fef3c7',
    textMuted: '#fcd34d',
  },
  'skipped': {
    border: 'rgba(255,255,255,0.06)',
    bgTop: 'rgba(38,38,42,0.6)',
    bgBottom: 'rgba(28,28,32,0.6)',
    accent: '#52525b',
    glow: 'rgba(0,0,0,0)',
    text: '#a1a1aa',
    textMuted: '#71717a',
  },
  'cached': {
    border: 'rgba(139,92,246,0.40)',
    bgTop: 'rgba(40,28,60,0.85)',
    bgBottom: 'rgba(28,20,46,0.85)',
    accent: '#8b5cf6',
    glow: 'rgba(139,92,246,0.25)',
    text: '#ede9fe',
    textMuted: '#c4b5fd',
  },
};

/** Border-only map for GroupNode. */
export const stateBorderColors: Record<string, string> = Object.fromEntries(
  Object.entries(stateColors).map(([k, v]) => [k, v.border])
);

/** Selection ring — used by all node types when selected/highlighted. */
export const selectionRing = {
  color: '#22d3ee',           // cyan-400
  glow: 'rgba(34,211,238,0.35)',
};

/**
 * Build a CSS background string for a node card.
 * Top→bottom soft gradient + ultra-subtle inner highlight on top edge.
 */
export function nodeBackground(s: NodeStateStyle): string {
  return [
    `linear-gradient(rgba(255,255,255,0.04), rgba(255,255,255,0) 28%)`,
    `linear-gradient(180deg, ${s.bgTop} 0%, ${s.bgBottom} 100%)`,
  ].join(', ');
}

/**
 * Build a layered box-shadow for a node card.
 *  - Selected: cyan ring + soft cyan glow.
 *  - Glowing state (running/success/error): tinted halo + crisp shadow.
 *  - Default: subtle floating shadow + 1px inset highlight on top.
 */
export function nodeShadow(s: NodeStateStyle, selected: boolean): string {
  const inset = `inset 0 1px 0 rgba(255,255,255,0.05)`;
  if (selected) {
    return [
      `0 0 0 1.5px ${selectionRing.color}`,
      `0 0 16px 0 ${selectionRing.glow}`,
      `0 4px 12px rgba(0,0,0,0.35)`,
      inset,
    ].join(', ');
  }
  const halo = s.glow === 'rgba(0,0,0,0)' ? '' : `0 0 0 1px ${s.glow}, `;
  return `${halo}0 1px 2px rgba(0,0,0,0.30), 0 4px 12px rgba(0,0,0,0.20), ${inset}`;
}
