import type { MarkerKind } from "./layout";

const SHAPES: Record<MarkerKind, string> = {
  pause_request: "M-4.5,-4 L4.5,-4 L0,4.5 Z",
  blocked: "M-4,-4 H4 V4 H-4 Z",
  snapshot: "M0,-5.5 L5.5,0 L0,5.5 L-5.5,0 Z",
  resume: "M-3.5,-5 L5,0 L-3.5,5 Z",
  fork: "M0,0 m-4.5,0 a4.5,4.5 0 1,0 9,0 a4.5,4.5 0 1,0 -9,0 Z",
};

export const MARKER_LABELS: Record<MarkerKind, string> = {
  pause_request: "pause request",
  blocked: "blocked attempt",
  snapshot: "snapshot",
  resume: "resume",
  fork: "fork",
};

/** The shape of a marker, centered on the origin. Shape and color both encode the kind. */
export function MarkerGlyph({ kind }: { kind: MarkerKind }) {
  return (
    <>
      <path className="ring" d={SHAPES[kind]} strokeWidth={4} stroke="var(--surface)" strokeLinejoin="round" />
      <path className="shape" d={SHAPES[kind]} strokeLinejoin="round" />
      {kind === "blocked" && <path className="x" d="M-2,-2 L2,2 M2,-2 L-2,2" strokeLinecap="round" />}
    </>
  );
}

export function LegendMarker({ kind }: { kind: MarkerKind }) {
  return (
    <svg width={13} height={13} viewBox="-6.5 -6.5 13 13" aria-hidden="true">
      <g className="tl-marker" data-kind={kind}>
        <MarkerGlyph kind={kind} />
      </g>
    </svg>
  );
}
