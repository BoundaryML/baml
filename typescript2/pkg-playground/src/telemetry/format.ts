/** Formatting and small derivations shared by the Telemetry views. */

import type { SpanNode, TelemetryStatus } from './evidence';

export function formatDuration(ms: number | null): string {
  if (ms == null) return '';
  if (ms >= 60_000) {
    const minutes = Math.floor(ms / 60_000);
    const seconds = Math.floor((ms % 60_000) / 1000);
    return `${minutes}m ${seconds}s`;
  }
  if (ms >= 1000) return `${(ms / 1000).toFixed(ms >= 10_000 ? 1 : 2)}s`;
  if (ms >= 1) return `${Math.round(ms)}ms`;
  return `${ms.toFixed(2)}ms`;
}

export function formatCount(value: number | null): string {
  return value == null ? '' : value.toLocaleString();
}

/** HH:MM:SS.mmm on the execution's own clock. */
export function formatClock(epochMs: number): string {
  const date = new Date(epochMs);
  const hh = String(date.getHours()).padStart(2, '0');
  const mm = String(date.getMinutes()).padStart(2, '0');
  const ss = String(date.getSeconds()).padStart(2, '0');
  const ms = String(date.getMilliseconds()).padStart(3, '0');
  return `${hh}:${mm}:${ss}.${ms}`;
}

export function formatTimeOfDay(epochMs: number): string {
  return new Date(epochMs).toLocaleTimeString([], { hour12: false });
}

/** Boundary brand purple. Model calls own this colour exclusively. */
export const LLM_PURPLE = '#8b5cf6';

/**
 * Colour for a function.
 *
 * Ordinary calls get a quiet blue-grey whose lightness varies by name, so
 * neighbouring bars stay tellable apart without competing with the two
 * colours that carry meaning: purple for model calls, and green/red which
 * are reserved for status.
 */
export function functionColor(name: string, kind: string): string {
  if (kind === 'llm') return LLM_PURPLE;
  let hash = 0;
  for (let index = 0; index < name.length; index += 1) {
    hash = (hash * 31 + name.charCodeAt(index)) | 0;
  }
  const hue = 214 + (Math.abs(hash) % 16);
  const saturation = 12 + (Math.abs(hash >> 8) % 10);
  const lightness = 42 + (Math.abs(hash >> 16) % 20);
  return `hsl(${hue} ${saturation}% ${lightness}%)`;
}

export function statusStyles(status: TelemetryStatus): string {
  if (status === 'failed') {
    return 'text-vsc-red bg-vsc-red/10 border-vsc-red/25';
  }
  if (status === 'running') {
    return 'text-vsc-accent bg-vsc-accent/10 border-vsc-accent/25';
  }
  if (status === 'cancelled') {
    return 'text-vsc-yellow bg-vsc-yellow/10 border-vsc-yellow/25';
  }
  return 'text-vsc-green bg-vsc-green/10 border-vsc-green/25';
}

/**
 * What a set of retained spans can say about a call path's durations.
 *
 * The store keeps no duration histogram, so any distribution has to come
 * from the calls that were individually retained. When those are all of
 * them the quantiles describe the population and can be named as such. When
 * they are a policy-chosen subset they describe only the sample, and naming
 * a p90 over two retained calls out of forty-one would be a fabrication --
 * so that case reports a range and says how many calls it covers.
 */
export type DurationSummary =
  | { kind: 'none' }
  | {
      kind: 'population';
      count: number;
      p50: number;
      p90: number;
      p99: number;
    }
  | {
      kind: 'sample';
      retained: number;
      total: number;
      min: number;
      median: number;
      max: number;
    };

export function summarizeDurations(
  spans: SpanNode[],
  totalCalls: number,
): DurationSummary {
  const durations = spans
    .map((span) => span.durationMs)
    .filter((value): value is number => value != null)
    .sort((left, right) => left - right);
  if (durations.length === 0) return { kind: 'none' };

  const at = (fraction: number): number => {
    const index = Math.min(
      durations.length - 1,
      Math.max(0, Math.ceil(fraction * durations.length) - 1),
    );
    return durations[index];
  };

  if (durations.length >= totalCalls) {
    return {
      count: durations.length,
      kind: 'population',
      p50: at(0.5),
      p90: at(0.9),
      p99: at(0.99),
    };
  }
  return {
    kind: 'sample',
    max: durations.at(-1) ?? durations[0],
    median: at(0.5),
    min: durations[0],
    retained: durations.length,
    total: totalCalls,
  };
}
