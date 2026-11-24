import { ALT_LOGO, ALT_MARK } from '@/components/home/boundary-alt';
import { GATE_LOGO, GATE_MARK } from '@/components/home/boundary-gate';
import { LAMB_PATHS } from '@/components/home/lamb';

const ALT_MARK_HEIGHT = 144;

// The Boundary marks, drawn from the brand's primary logo: the gate and the lowercase wordmark. Both
// use currentColor, so they follow the text color they sit in.

// The gate on its own.
export function BoundaryMark({ className }: { className?: string }) {
  return (
    // biome-ignore lint/a11y/noSvgWithoutTitle: decorative, the text beside it names it
    <svg
      aria-hidden
      className={className}
      fill="currentColor"
      viewBox={GATE_MARK.viewBox}
    >
      {GATE_MARK.paths.map((d) => (
        <path d={d} key={d} />
      ))}
    </svg>
  );
}

// The full logo: the gate and the lowercase wordmark.
export function BoundaryLogo({ className }: { className?: string }) {
  return (
    <svg
      aria-label="Boundary"
      className={className}
      fill="currentColor"
      role="img"
      viewBox={GATE_LOGO.viewBox}
    >
      {GATE_LOGO.paths.map((d) => (
        <path d={d} key={d} />
      ))}
    </svg>
  );
}

// The combined mark for Boundary and BAML: the alt mark's lower piece with the BAML lamb mid-leap
// in place of its two upper bars. The lamb sits low enough that the piece overlaps its hind legs,
// so the piece's own top edge cuts the lamb off and the two read as one shape. The lamb is the
// standalone BAML logomark; it keeps its pixel grid square and sits flush with the piece's left
// edge. The site does not use it outside /logo-ideas.
export const LAMB_MARK = {
  /** The mark's box in the alt mark's coordinates: 86 wide, from y -32 to y 144. */
  height: ALT_MARK_HEIGHT + 32,
  /** The lamb's size against its own 356 by 422 artwork. */
  lambScale: 0.24,
  /** How far the lamb rises above the alt mark's own box. */
  lift: 32,
  /** The alt mark's lower piece. It spans y 52 to 144. */
  piece: ALT_LOGO.paths[2],
  width: 86,
} as const;

function LambMarkParts() {
  return (
    <>
      <g
        transform={`translate(0 ${-LAMB_MARK.lift}) scale(${LAMB_MARK.lambScale})`}
      >
        {LAMB_PATHS.map((d) => (
          <path d={d} key={d} />
        ))}
      </g>
      <path d={LAMB_MARK.piece} />
    </>
  );
}

// The combined mark on its own.
export function BoundaryLambMark({ className }: { className?: string }) {
  return (
    // biome-ignore lint/a11y/noSvgWithoutTitle: decorative, the text beside it names it
    <svg
      aria-hidden
      className={className}
      fill="currentColor"
      viewBox={`0 ${-LAMB_MARK.lift} ${LAMB_MARK.width} ${LAMB_MARK.height}`}
    >
      <LambMarkParts />
    </svg>
  );
}

// The combined mark with the wordmark.
export function BoundaryLambLogo({ className }: { className?: string }) {
  return (
    <svg
      aria-label="Boundary"
      className={className}
      fill="currentColor"
      role="img"
      viewBox={`0 ${-LAMB_MARK.lift} ${ALT_LOGO.width} ${LAMB_MARK.height}`}
    >
      <LambMarkParts />
      {ALT_LOGO.paths.slice(ALT_MARK.paths.length).map((d) => (
        <path d={d} key={d} />
      ))}
    </svg>
  );
}
