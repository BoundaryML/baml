/**
 * Shared state colors & visual tokens for the graph.
 *
 * The graph is theme-aware (see ./theme.ts):
 *  - Light is the warm "paper" aesthetic the playground panel ships with
 *    (off-whites, ink text, tan hairlines — the BAML site deck family).
 *  - Dark is an ink surface: cards are elevated zinc panels with light text,
 *    state encoded by accent + a gentle tint, frames visible against #1f1f1f.
 *
 * In both themes:
 *  - Cards sit ON the canvas with a visible hairline.
 *  - State is encoded via the accent (icon chip / left rail) plus a gentle
 *    background tint, not heavy borders.
 *  - Nothing glows unless it is running/terminal.
 */
import type { GraphTheme } from './theme';

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

const stateColorsLight: Record<string, NodeStateStyle> = {
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

const stateColorsDark: Record<string, NodeStateStyle> = {
  'not-started': {
    border: 'rgba(255,255,255,0.14)',
    bgTop: '#2A2A2E',
    bgBottom: '#222226',
    accent: '#71717A',
    glow: 'rgba(0,0,0,0)',
    text: '#F4F4F5',
    textMuted: '#A1A1AA',
  },
  running: {
    border: 'rgba(96,165,250,0.55)',
    bgTop: '#1D2A44',
    bgBottom: '#172033',
    accent: '#3B82F6',
    glow: 'rgba(59,130,246,0.30)',
    text: '#F4F4F5',
    textMuted: '#93C5FD',
  },
  success: {
    border: 'rgba(74,222,128,0.45)',
    bgTop: '#16291F',
    bgBottom: '#122019',
    accent: '#22C55E',
    glow: 'rgba(34,197,94,0.22)',
    text: '#F4F4F5',
    textMuted: '#86EFAC',
  },
  error: {
    border: 'rgba(248,113,113,0.50)',
    bgTop: '#2E1A1C',
    bgBottom: '#251417',
    accent: '#EF4444',
    glow: 'rgba(239,68,68,0.22)',
    text: '#F4F4F5',
    textMuted: '#FCA5A5',
  },
  cancelled: {
    border: 'rgba(251,146,60,0.45)',
    bgTop: '#2C2012',
    bgBottom: '#231A0E',
    accent: '#F97316',
    glow: 'rgba(249,115,22,0.20)',
    text: '#F4F4F5',
    textMuted: '#FDBA74',
  },
  pending: {
    border: 'rgba(251,191,36,0.42)',
    bgTop: '#2C2012',
    bgBottom: '#231A0E',
    accent: '#F59E0B',
    glow: 'rgba(245,158,11,0.22)',
    text: '#F4F4F5',
    textMuted: '#FCD34D',
  },
  skipped: {
    border: 'rgba(255,255,255,0.08)',
    bgTop: 'rgba(39,39,42,0.50)',
    bgBottom: 'rgba(31,31,35,0.50)',
    accent: '#52525B',
    glow: 'rgba(0,0,0,0)',
    text: '#71717A',
    textMuted: '#52525B',
  },
  cached: {
    border: 'rgba(167,139,250,0.45)',
    bgTop: '#241A33',
    bgBottom: '#1C1529',
    accent: '#8B5CF6',
    glow: 'rgba(139,92,246,0.20)',
    text: '#F4F4F5',
    textMuted: '#C4B5FD',
  },
};

export function getStateColors(theme: GraphTheme): Record<string, NodeStateStyle> {
  return theme === 'dark' ? stateColorsDark : stateColorsLight;
}

/** Convenience: the per-state style for one state, with a safe fallback. */
export function stateStyle(theme: GraphTheme, state: string): NodeStateStyle {
  const map = getStateColors(theme);
  return map[state] ?? map['not-started']!;
}

/** A domain accent (LLM violet, loop cyan, branch amber). */
export interface DomainAccent {
  /** Solid color for the icon chip background fill. */
  accent: string;
  /** Card border tint for this domain (overrides state border). */
  border: string;
  /** Chip/badge background. */
  chipBg: string;
  /** Chip/badge foreground. */
  chipText: string;
  /** Chip/badge inset ring. */
  chipRing: string;
  /** Icon glyph color. */
  icon: string;
}

/** Non-state chrome: selection ring, group frames, domain accents, canvas. */
export interface GraphChrome {
  selectionRing: { color: string; glow: string };
  /** Dashed frame color for an idle (not-yet-run) subgraph. */
  groupBorderIdle: string;
  groupLabelText: string;
  groupLabelBg: string;
  groupLabelBorder: string;
  groupLabelTextSelected: string;
  groupLabelBgSelected: string;
  groupLabelShadow: string;
  iterationBg: string;
  iterationText: string;
  iterationBorder: string;
  /** Background dot color for the canvas. */
  backgroundDots: string;
  llm: DomainAccent;
  loop: DomainAccent;
  branch: DomainAccent;
  /** Layout-direction toggle button. */
  button: {
    bg: string;
    bgHover: string;
    border: string;
    borderHover: string;
    text: string;
    shadow: string;
  };
}

const chromeLight: GraphChrome = {
  selectionRing: { color: '#2563EB', glow: 'rgba(37,99,235,0.22)' },
  groupBorderIdle: 'rgba(26,22,18,0.28)',
  groupLabelText: '#1A1612',
  groupLabelBg: 'rgba(255,253,246,0.94)',
  groupLabelBorder: '#D8CFBD',
  groupLabelTextSelected: '#1D4ED8',
  groupLabelBgSelected: 'rgba(234,241,254,0.94)',
  groupLabelShadow: '0 1px 2px rgba(26,22,18,0.12)',
  iterationBg: 'rgba(37,99,235,0.10)',
  iterationText: '#1D4ED8',
  iterationBorder: 'rgba(59,130,246,0.30)',
  backgroundDots: 'rgba(42,37,32,0.12)',
  llm: {
    accent: '#6D28D9',
    border: 'rgba(109,40,217,0.40)',
    chipBg: 'rgba(109,40,217,0.10)',
    chipText: '#6D28D9',
    chipRing: 'rgba(109,40,217,0.30)',
    icon: '#6D28D9',
  },
  loop: {
    accent: 'rgba(14,165,233,0.15)',
    border: 'rgba(14,165,233,0.35)',
    chipBg: 'rgba(14,165,233,0.15)',
    chipText: '#0369A1',
    chipRing: 'rgba(14,165,233,0.35)',
    icon: '#0284C7',
  },
  branch: {
    accent: 'rgba(245,158,11,0.15)',
    border: 'rgba(245,158,11,0.35)',
    chipBg: 'rgba(245,158,11,0.15)',
    chipText: '#B45309',
    chipRing: 'rgba(245,158,11,0.35)',
    icon: '#D97706',
  },
  button: {
    bg: 'rgba(255,253,246,0.92)',
    bgHover: '#F4EEE0',
    border: '#D8CFBD',
    borderHover: '#C9BFA9',
    text: '#1A1612',
    shadow:
      '0 1px 2px rgba(26,22,18,0.10), inset 0 1px 0 rgba(255,255,255,0.6)',
  },
};

const chromeDark: GraphChrome = {
  selectionRing: { color: '#60A5FA', glow: 'rgba(96,165,250,0.30)' },
  groupBorderIdle: 'rgba(255,255,255,0.20)',
  groupLabelText: '#E4E4E7',
  groupLabelBg: 'rgba(39,39,42,0.92)',
  groupLabelBorder: 'rgba(255,255,255,0.14)',
  groupLabelTextSelected: '#93C5FD',
  groupLabelBgSelected: 'rgba(30,41,59,0.94)',
  groupLabelShadow: '0 1px 3px rgba(0,0,0,0.45)',
  iterationBg: 'rgba(96,165,250,0.16)',
  iterationText: '#93C5FD',
  iterationBorder: 'rgba(96,165,250,0.35)',
  backgroundDots: 'rgba(255,255,255,0.10)',
  llm: {
    accent: '#8B5CF6',
    border: 'rgba(139,92,246,0.50)',
    chipBg: 'rgba(139,92,246,0.18)',
    chipText: '#C4B5FD',
    chipRing: 'rgba(139,92,246,0.45)',
    icon: '#C4B5FD',
  },
  loop: {
    accent: 'rgba(56,189,248,0.20)',
    border: 'rgba(56,189,248,0.45)',
    chipBg: 'rgba(56,189,248,0.18)',
    chipText: '#7DD3FC',
    chipRing: 'rgba(56,189,248,0.45)',
    icon: '#7DD3FC',
  },
  branch: {
    accent: 'rgba(251,191,36,0.20)',
    border: 'rgba(251,191,36,0.45)',
    chipBg: 'rgba(251,191,36,0.18)',
    chipText: '#FCD34D',
    chipRing: 'rgba(251,191,36,0.45)',
    icon: '#FCD34D',
  },
  button: {
    bg: 'rgba(39,39,42,0.92)',
    bgHover: '#3F3F46',
    border: 'rgba(255,255,255,0.12)',
    borderHover: 'rgba(255,255,255,0.22)',
    text: '#E4E4E7',
    shadow:
      '0 2px 8px rgba(0,0,0,0.40), inset 0 1px 0 rgba(255,255,255,0.06)',
  },
};

export function getChrome(theme: GraphTheme): GraphChrome {
  return theme === 'dark' ? chromeDark : chromeLight;
}

/**
 * Build a CSS background string for a node card.
 * A soft sheen on the top edge + a top→bottom paper/ink gradient.
 */
export function nodeBackground(s: NodeStateStyle, theme: GraphTheme): string {
  const sheen =
    theme === 'dark'
      ? 'linear-gradient(rgba(255,255,255,0.06), rgba(255,255,255,0) 35%)'
      : 'linear-gradient(rgba(255,255,255,0.55), rgba(255,255,255,0) 30%)';
  return [
    sheen,
    `linear-gradient(180deg, ${s.bgTop} 0%, ${s.bgBottom} 100%)`,
  ].join(', ');
}

/**
 * Build a layered box-shadow for a node card.
 *  - Selected: ring + soft halo.
 *  - Glowing state (running/success/error): tinted halo + drop shadow.
 *  - Default: soft floating shadow + bright inset top edge.
 */
export function nodeShadow(
  s: NodeStateStyle,
  selected: boolean,
  theme: GraphTheme,
): string {
  const ring = getChrome(theme).selectionRing;
  if (theme === 'dark') {
    const inset = `inset 0 1px 0 rgba(255,255,255,0.06)`;
    if (selected) {
      return [
        `0 0 0 1.5px ${ring.color}`,
        `0 0 16px 0 ${ring.glow}`,
        `0 4px 14px rgba(0,0,0,0.45)`,
        inset,
      ].join(', ');
    }
    const halo = s.glow === 'rgba(0,0,0,0)' ? '' : `0 0 0 1px ${s.glow}, `;
    return `${halo}0 1px 2px rgba(0,0,0,0.35), 0 4px 12px rgba(0,0,0,0.40), ${inset}`;
  }

  const inset = `inset 0 1px 0 rgba(255,255,255,0.65)`;
  if (selected) {
    return [
      `0 0 0 1.5px ${ring.color}`,
      `0 0 14px 0 ${ring.glow}`,
      `0 3px 10px rgba(26,22,18,0.12)`,
      inset,
    ].join(', ');
  }
  const halo = s.glow === 'rgba(0,0,0,0)' ? '' : `0 0 0 1px ${s.glow}, `;
  return `${halo}0 1px 2px rgba(26,22,18,0.08), 0 3px 10px rgba(26,22,18,0.07), ${inset}`;
}
