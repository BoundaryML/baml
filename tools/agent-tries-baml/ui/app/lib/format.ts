// Shared formatting helpers for the dashboard UI.

/**
 * Formats an elapsed duration in milliseconds as a compact relative age,
 * scaling through seconds, minutes, hours, days, weeks, months, and years
 * (e.g. "45s", "12m", "3h", "2d", "5w", "4mo", "1y").
 * @param ms - elapsed milliseconds
 * @returns a short relative-age string
 */
export function ago(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d}d`;
  const w = Math.floor(d / 7);
  if (w < 9) return `${w}w`;
  const mo = Math.floor(d / 30.4);
  if (mo < 12) return `${mo}mo`;
  return `${Math.floor(d / 365.25) || 1}y`;
}
