import type { RunStatus, Site } from "../protocol";

/** Status is never shown by color alone. The runs list reuses these for its rollup. */
export const GLYPHS: Record<RunStatus, string> = {
  starting: "…",
  running: "▶",
  pausing: "‖",
  paused: "‖",
  completed: "✓",
  failed: "✕",
  lost: "✕",
  cancelled: "⊘",
  migrated: "⇢",
  sleeping: "☾",
};

/** Status is never shown by color alone: every badge has a glyph and a label. */
export function StatusBadge({ status }: { status: RunStatus }) {
  return (
    <span className="status" data-status={status}>
      <span className="glyph" aria-hidden="true">{GLYPHS[status] ?? "?"}</span>
      {status}
    </span>
  );
}

export function SiteChip({ site }: { site: Site }) {
  return (
    <span className="site-chip">
      <i className="site-dot" data-site={site} />
      {site}
    </span>
  );
}
