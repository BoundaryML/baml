/**
 * Shared state colors & visual tokens for the graph.
 *
 * The palette targets the light "paper" aesthetic the playground panel ships
 * with (warm off-whites, ink text, tan hairlines — the same family as the
 * BAML site decks):
 *  - Cards are paper with a visible hairline; they sit ON the canvas.
 *  - State is encoded via the accent (left rail / icon chip) plus a gentle
 *    background tint, not heavy borders.
 *  - Shadows are soft and warm; nothing glows unless it is running.
 */
export interface NodeStateStyle {
  /** Tinted hairline border; the node frame. */
  border: string;
  /** Top-of-gradient background — the brighter face of the card. */
  bgTop: string;
  /** Bottom-of-gradient background — the deeper face. */
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
    border: '#D8CFBD',
    bgTop: '#FFFDF6',
    bgBottom: '#F8F2E4',
    accent: '#8A8580',
    glow: 'rgba(0,0,0,0)',
    text: '#1A1612',
    textMuted: '#6F6A63',
  },
  running: {
    border: 'rgba(37,99,235,0.50)',
    bgTop: '#F4F8FF',
    bgBottom: '#E8F0FD',
    accent: '#2563EB',
    glow: 'rgba(37,99,235,0.20)',
    text: '#1A1612',
    textMuted: '#1D4ED8',
  },
  success: {
    border: 'rgba(4,120,87,0.45)',
    bgTop: '#F0FAF4',
    bgBottom: '#E1F3E9',
    accent: '#047857',
    glow: 'rgba(4,120,87,0.16)',
    text: '#1A1612',
    textMuted: '#047857',
  },
  error: {
    border: 'rgba(180,35,24,0.45)',
    bgTop: '#FDF1EF',
    bgBottom: '#F9E3DF',
    accent: '#B42318',
    glow: 'rgba(180,35,24,0.16)',
    text: '#1A1612',
    textMuted: '#B42318',
  },
  cancelled: {
    border: 'rgba(180,83,9,0.45)',
    bgTop: '#FCF4E4',
    bgBottom: '#F7EAD2',
    accent: '#B45309',
    glow: 'rgba(180,83,9,0.15)',
    text: '#1A1612',
    textMuted: '#B45309',
  },
  pending: {
    border: 'rgba(180,83,9,0.40)',
    bgTop: '#FCF4E4',
    bgBottom: '#F7EAD2',
    accent: '#D97706',
    glow: 'rgba(217,119,6,0.16)',
    text: '#1A1612',
    textMuted: '#B45309',
  },
  skipped: {
    border: 'rgba(26,22,18,0.10)',
    bgTop: 'rgba(251,247,237,0.65)',
    bgBottom: 'rgba(244,238,224,0.65)',
    accent: '#B9B2A3',
    glow: 'rgba(0,0,0,0)',
    text: '#8A8580',
    textMuted: '#B9B2A3',
  },
  cached: {
    border: 'rgba(109,40,217,0.40)',
    bgTop: '#F6F1FE',
    bgBottom: '#EDE4FB',
    accent: '#6D28D9',
    glow: 'rgba(109,40,217,0.14)',
    text: '#1A1612',
    textMuted: '#6D28D9',
  },
};

/** Border-only map for GroupNode. */
export const stateBorderColors: Record<string, string> = Object.fromEntries(
  Object.entries(stateColors).map(([k, v]) => [k, v.border]),
);

/** Selection ring — keyword blue; distinct from the green success state. */
export const selectionRing = {
  color: '#2563EB',
  glow: 'rgba(37,99,235,0.22)',
};

/**
 * Build a CSS background string for a node card.
 * Top→bottom paper gradient + a soft white sheen on the top edge.
 */
export function nodeBackground(s: NodeStateStyle): string {
  return [
    `linear-gradient(rgba(255,255,255,0.55), rgba(255,255,255,0) 30%)`,
    `linear-gradient(180deg, ${s.bgTop} 0%, ${s.bgBottom} 100%)`,
  ].join(', ');
}

/**
 * Build a layered box-shadow for a node card.
 *  - Selected: blue ring + soft blue halo.
 *  - Glowing state (running/success/error): tinted halo + warm shadow.
 *  - Default: soft warm floating shadow + bright inset top edge.
 */
export function nodeShadow(s: NodeStateStyle, selected: boolean): string {
  const inset = `inset 0 1px 0 rgba(255,255,255,0.65)`;
  if (selected) {
    return [
      `0 0 0 1.5px ${selectionRing.color}`,
      `0 0 14px 0 ${selectionRing.glow}`,
      `0 3px 10px rgba(26,22,18,0.12)`,
      inset,
    ].join(', ');
  }
  const halo = s.glow === 'rgba(0,0,0,0)' ? '' : `0 0 0 1px ${s.glow}, `;
  return `${halo}0 1px 2px rgba(26,22,18,0.08), 0 3px 10px rgba(26,22,18,0.07), ${inset}`;
}
