// Shared formatting helpers for the dashboard UI.

/**
 * Formats an elapsed duration in milliseconds as a compact relative age.
 * @param ms - elapsed milliseconds
 * @returns a short string like "45s", "12m", or "3h"
 */
export function ago(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.round(s / 60);
  return m < 60 ? `${m}m` : `${Math.round(m / 60)}h`;
}
