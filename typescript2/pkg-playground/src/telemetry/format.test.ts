import { describe, expect, it } from 'vitest';

import type { SpanNode } from './evidence';
import { formatDuration, summarizeDurations } from './format';

function span(durationMs: number | null): SpanNode {
  return {
    contextId: 'p1',
    durationMs,
    errorId: null,
    fn: 'Fn',
    id: `s${durationMs}`,
    kind: 'baml',
    parentId: null,
    reasons: [],
    source: null,
    startMs: 0,
    status: 'succeeded',
    threadId: 't',
    threadName: 'main',
    values: {
      args: {
        availability: { state: 'notCaptured' },
        body: null,
        mediaCid: null,
      },
      error: {
        availability: { state: 'notApplicable' },
        body: null,
        mediaCid: null,
      },
      output: {
        availability: { state: 'notCaptured' },
        body: null,
        mediaCid: null,
      },
    },
  };
}

describe('formatDuration', () => {
  it('scales units by magnitude', () => {
    expect(formatDuration(0.25)).toBe('0.25ms');
    expect(formatDuration(84)).toBe('84ms');
    expect(formatDuration(1840)).toBe('1.84s');
    expect(formatDuration(252_000)).toBe('4m 12s');
  });

  it('renders an absent duration as empty, never as zero', () => {
    expect(formatDuration(null)).toBe('');
  });
});

describe('summarizeDurations', () => {
  it('names quantiles only when every call was retained', () => {
    const spans = [10, 20, 30, 40].map(span);
    expect(summarizeDurations(spans, 4)).toEqual({
      count: 4,
      kind: 'population',
      p50: 20,
      p90: 40,
      p99: 40,
    });
  });

  it('reports a range, not a p90, when only a sample was retained', () => {
    // A p90 over two retained calls out of forty-one would be invented.
    const spans = [11, 14].map(span);
    expect(summarizeDurations(spans, 41)).toEqual({
      kind: 'sample',
      max: 14,
      median: 11,
      min: 11,
      retained: 2,
      total: 41,
    });
  });

  it('says nothing when no call was retained', () => {
    expect(summarizeDurations([], 41)).toEqual({ kind: 'none' });
  });

  it('ignores spans that never ended', () => {
    expect(summarizeDurations([span(null)], 1)).toEqual({ kind: 'none' });
  });
});
