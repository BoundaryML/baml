/**
 * Shared state color constants for graph nodes.
 *
 * The full shape includes all keys used across different node types:
 * - border: node border color (all node types)
 * - bg: node background color (BaseNode, LLMNode, DiamondNode)
 * - accent: status icon color (BaseNode) / badge color (LLMNode)
 */
export const stateColors: Record<
  string,
  { border: string; bg: string; accent: string }
> = {
  'not-started': { border: '#3c3c3c', bg: '#252526', accent: '#4a4a4a' },
  'running':     { border: '#2563eb', bg: '#1e293b', accent: '#2563eb' },
  'success':     { border: '#16a34a', bg: '#14281e', accent: '#16a34a' },
  'error':       { border: '#dc2626', bg: '#2a1515', accent: '#dc2626' },
  'pending':     { border: '#d97706', bg: '#252526', accent: '#d97706' },
  'skipped':     { border: '#6b7280', bg: '#1f1f1f', accent: '#6b7280' },
  'cached':      { border: '#7c3aed', bg: '#1e1528', accent: '#7c3aed' },
};

/** Border-only map for GroupNode. */
export const stateBorderColors: Record<string, string> = Object.fromEntries(
  Object.entries(stateColors).map(([k, v]) => [k, v.border])
);
