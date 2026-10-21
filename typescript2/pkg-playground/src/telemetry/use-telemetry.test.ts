import { describe, expect, it } from 'vitest';

import { pollInterval } from './use-telemetry';

describe('pollInterval', () => {
  const now = 100 * 60_000;
  const minutesAgo = (minutes: number) => now - minutes * 60_000;

  it('polls fast while a run may still be in flight', () => {
    expect(pollInterval([{ startedMs: null, status: 'running' }], now)).toBe(
      2000,
    );
    expect(
      pollInterval([{ startedMs: minutesAgo(1), status: 'incomplete' }], now),
    ).toBe(2000);
  });

  it('polls rarely once an incomplete run is old or has no start', () => {
    expect(
      pollInterval(
        [
          { startedMs: minutesAgo(30), status: 'incomplete' },
          { startedMs: null, status: 'incomplete' },
          { startedMs: minutesAgo(1), status: 'succeeded' },
        ],
        now,
      ),
    ).toBe(30_000);
  });

  it('stops when nothing can change', () => {
    expect(
      pollInterval(
        [
          { startedMs: minutesAgo(1), status: 'succeeded' },
          { startedMs: minutesAgo(2), status: 'failed' },
        ],
        now,
      ),
    ).toBeNull();
  });
});
