/**
 * The clock that the timeline uses for "now". Fixture playback replaces it
 * with a virtual clock, so recorded timestamps keep their meaning at any
 * playback speed.
 */
let source: () => number = () => Date.now();

export const clock = {
  now(): number {
    return source();
  },
  set(next: () => number): void {
    source = next;
  },
  reset(): void {
    source = () => Date.now();
  },
};
