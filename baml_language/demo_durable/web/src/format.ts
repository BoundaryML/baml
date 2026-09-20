export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return "n/a";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10240 ? 2 : 1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

export function formatMs(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return "n/a";
  const abs = Math.abs(ms);
  if (abs < 10) return `${ms.toFixed(2)} ms`;
  if (abs < 1000) return `${ms.toFixed(abs < 100 ? 1 : 0)} ms`;
  if (abs < 60_000) return `${(ms / 1000).toFixed(2)} s`;
  const minutes = Math.floor(ms / 60_000);
  return `${minutes} min ${Math.round((ms - minutes * 60_000) / 1000)} s`;
}

export function formatCount(value: number | null | undefined): string {
  return value === null || value === undefined ? "n/a" : value.toLocaleString("en-US");
}

const pad = (value: number, width: number): string => String(value).padStart(width, "0");

/** Local wall-clock time with milliseconds, for example `17:00:04.556`. */
export function formatClock(ts: number): string {
  const date = new Date(ts);
  return `${pad(date.getHours(), 2)}:${pad(date.getMinutes(), 2)}:${pad(date.getSeconds(), 2)}.${pad(date.getMilliseconds(), 3)}`;
}

/** Offset from the start of the timeline, for example `+4.56s`. */
export function formatOffset(ms: number, stepMs = 10): string {
  const digits = stepMs >= 1000 ? 0 : stepMs >= 100 ? 1 : stepMs >= 10 ? 2 : 3;
  return `+${(ms / 1000).toFixed(digits)}s`;
}
