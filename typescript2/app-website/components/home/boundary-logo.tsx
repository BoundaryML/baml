import { COMBINATION, WORDMARK } from '@/components/home/lamb';

// The Boundary marks, drawn as the brand's combination mark (the lamb leaping over the gate)
// instead of the bare gate. Both use currentColor, so they follow the text color they sit in.

// The combination mark on its own.
export function BoundaryMark({ className }: { className?: string }) {
  return (
    // biome-ignore lint/a11y/noSvgWithoutTitle: decorative, the text beside it names it
    <svg
      aria-hidden
      className={className}
      fill="currentColor"
      viewBox={COMBINATION.viewBox}
    >
      <path d={COMBINATION.gate} />
      <path d={COMBINATION.lamb} />
    </svg>
  );
}

// The primary logo with the combination mark in the gate's place. The mark is scaled so its gate
// is exactly the primary logo's gate (137 wide), and the lamb rises above the wordmark.
const K = 137 / 474;
const LIFT = 212 * K;

export function BoundaryLogo({ className }: { className?: string }) {
  return (
    <svg
      aria-label="Boundary"
      className={className}
      fill="currentColor"
      role="img"
      viewBox={`0 ${-LIFT} 649 ${111 + LIFT}`}
    >
      <g transform={`translate(0 ${-LIFT}) scale(${K})`}>
        <path d={COMBINATION.gate} />
        <path d={COMBINATION.lamb} />
      </g>
      <path d={WORDMARK} />
    </svg>
  );
}
