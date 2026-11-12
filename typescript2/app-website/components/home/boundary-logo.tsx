import { ALT_LOGO, ALT_MARK } from '@/components/home/boundary-alt';
import { LAMB_PATHS } from '@/components/home/lamb';

const ALT_MARK_HEIGHT = 144;

// The Boundary marks, drawn from the brand's alt logo. Both use currentColor, so they follow the
// text color they sit in.

// The mark on its own.
export function BoundaryMark({ className }: { className?: string }) {
  return (
    // biome-ignore lint/a11y/noSvgWithoutTitle: decorative, the text beside it names it
    <svg
      aria-hidden
      className={className}
      fill="currentColor"
      viewBox={ALT_MARK.viewBox}
    >
      {ALT_MARK.paths.map((d) => (
        <path d={d} key={d} />
      ))}
    </svg>
  );
}

// The full logo: the mark and the lowercase wordmark.
export function BoundaryLogo({ className }: { className?: string }) {
  return (
    <svg
      aria-label="Boundary"
      className={className}
      fill="currentColor"
      role="img"
      viewBox={ALT_LOGO.viewBox}
    >
      {ALT_LOGO.paths.map((d) => (
        <path d={d} key={d} />
      ))}
    </svg>
  );
}

// The combined mark for Boundary and BAML: the alt mark's lower piece with the BAML lamb mid-leap
// in place of its two upper bars. The lamb sits low enough that the piece overlaps its hind legs,
// so the piece's own top edge cuts the lamb off and the two read as one shape. The lamb is the
// standalone BAML logomark; it keeps its pixel grid square and sits flush with the piece's left
// edge. This replaces the brand's older combination mark (the lamb over the gate) on the site.
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
