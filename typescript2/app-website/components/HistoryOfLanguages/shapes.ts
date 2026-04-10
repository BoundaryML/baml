// All shapes defined as builder functions called client-side (inside useEffect).
// Never call Math.random() at module scope — SSR safety.
// Coordinate system: 0..1 normalized, scaled to (canvasW, canvasH) at call time.

export type ShapeId =
  | 'sphinx'
  | 'colosseum'
  | 'cathedral'
  | 'goldengate'
  | 'burjkhalifa'
  | 'spaceelevator';

export type ShapePoint = { x: number; y: number };

// --- Parametric primitives ---

type Curve = (t: number) => ShapePoint;

function sampleCurve(fn: Curve, n: number): ShapePoint[] {
  return Array.from({ length: n }, (_, i) => fn(i / Math.max(1, n - 1)));
}

function seg(x0: number, y0: number, x1: number, y1: number): Curve {
  return (t) => ({ x: x0 + t * (x1 - x0), y: y0 + t * (y1 - y0) });
}

function arc(cx: number, cy: number, rx: number, ry: number, a0: number, a1: number): Curve {
  return (t) => {
    const a = a0 + t * (a1 - a0);
    return { x: cx + Math.cos(a) * rx, y: cy + Math.sin(a) * ry };
  };
}

function quad(x0: number, y0: number, cx: number, cy: number, x1: number, y1: number): Curve {
  return (t) => {
    const u = 1 - t;
    return {
      x: u * u * x0 + 2 * u * t * cx + t * t * x1,
      y: u * u * y0 + 2 * u * t * cy + t * t * y1,
    };
  };
}

function scatterAround(pts: ShapePoint[], jitter: number): ShapePoint[] {
  return pts.map((p) => ({
    x: p.x + (Math.random() - 0.5) * jitter,
    y: p.y + (Math.random() - 0.5) * jitter,
  }));
}

function scale(pts: ShapePoint[], w: number, h: number): ShapePoint[] {
  return pts.map((p) => ({ x: p.x * w, y: p.y * h }));
}

function shuffle<T>(arr: T[]): T[] {
  const a = [...arr];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j]!, a[i]!];
  }
  return a;
}

const PI = Math.PI;
const TAU = PI * 2;

// --- Landmark builders ---
// All coordinates in 0..1 normalized space.
// Each builder returns exactly `count` points, shuffled.

function buildSphinx(count: number): ShapePoint[] {
  const raw: ShapePoint[] = [
    // Ground line
    ...sampleCurve(seg(0.02, 0.87, 0.98, 0.87), 32),
    // Left front paws (two rectangular columns)
    ...sampleCurve(seg(0.07, 0.87, 0.07, 0.70), 10),
    ...sampleCurve(seg(0.07, 0.70, 0.20, 0.70), 8),
    ...sampleCurve(seg(0.20, 0.70, 0.20, 0.87), 10),
    ...sampleCurve(seg(0.13, 0.70, 0.13, 0.82), 7), // divider between paws
    // Body top silhouette (gently arched)
    ...sampleCurve(quad(0.20, 0.67, 0.42, 0.52, 0.66, 0.60), 26),
    // Body bottom
    ...sampleCurve(quad(0.20, 0.80, 0.50, 0.80, 0.68, 0.82), 18),
    // Rear haunches
    ...sampleCurve(arc(0.74, 0.74, 0.09, 0.13, -PI * 0.65, PI * 0.35), 14),
    // Neck (two parallel lines)
    ...sampleCurve(seg(0.66, 0.60, 0.63, 0.40), 11),
    ...sampleCurve(seg(0.73, 0.60, 0.73, 0.40), 11),
    // Head: left side
    ...sampleCurve(seg(0.63, 0.40, 0.63, 0.15), 15),
    // Head: chin
    ...sampleCurve(seg(0.73, 0.40, 0.90, 0.40), 11),
    // Head: right side
    ...sampleCurve(seg(0.90, 0.40, 0.90, 0.22), 12),
    // Headdress: top edges
    ...sampleCurve(seg(0.90, 0.22, 0.76, 0.13), 9),
    ...sampleCurve(seg(0.76, 0.13, 0.63, 0.15), 8),
    // Headdress side stripes (fall from sides of head down)
    ...sampleCurve(seg(0.68, 0.15, 0.63, 0.38), 9),
    ...sampleCurve(seg(0.85, 0.15, 0.90, 0.38), 9),
    // Face details
    ...sampleCurve(seg(0.71, 0.27, 0.82, 0.27), 7), // eye line
    ...sampleCurve(seg(0.74, 0.33, 0.80, 0.33), 5), // nose
  ];
  return resample(scatterAround(raw, 0.006), count);
}

function buildColosseum(count: number): ShapePoint[] {
  const raw: ShapePoint[] = [
    // Outer ellipse
    ...sampleCurve(arc(0.50, 0.50, 0.46, 0.43, 0, TAU), 60),
    // Second tier
    ...sampleCurve(arc(0.50, 0.50, 0.35, 0.33, 0, TAU), 46),
    // Third tier
    ...sampleCurve(arc(0.50, 0.50, 0.24, 0.22, 0, TAU), 32),
    // Ground
    ...sampleCurve(seg(0.02, 0.94, 0.98, 0.94), 28),
    // Radial structural lines (pillars), 12 evenly spaced
    ...Array.from({ length: 12 }, (_, i) => {
      const a = (i / 12) * TAU;
      return sampleCurve(
        seg(
          0.50 + Math.cos(a) * 0.24,
          0.50 + Math.sin(a) * 0.22,
          0.50 + Math.cos(a) * 0.46,
          0.50 + Math.sin(a) * 0.43,
        ),
        5,
      );
    }).flat(),
    // Broken section: extra density on intact left/bottom arc to compensate
    ...sampleCurve(arc(0.50, 0.50, 0.46, 0.43, PI * 0.7, PI * 1.8), 20),
  ];
  return resample(scatterAround(raw, 0.007), count);
}

function buildCathedral(count: number): ShapePoint[] {
  // Cathedral fits in a narrow portrait band: x 0.22..0.78, y 0.03..0.95
  const raw: ShapePoint[] = [
    // Left tower: walls
    ...sampleCurve(seg(0.22, 0.95, 0.22, 0.20), 24),
    ...sampleCurve(seg(0.36, 0.95, 0.36, 0.20), 24),
    // Left tower spire
    ...sampleCurve(seg(0.22, 0.20, 0.29, 0.04), 8),
    ...sampleCurve(seg(0.36, 0.20, 0.29, 0.04), 8),
    // Right tower: walls
    ...sampleCurve(seg(0.64, 0.95, 0.64, 0.20), 24),
    ...sampleCurve(seg(0.78, 0.95, 0.78, 0.20), 24),
    // Right tower spire
    ...sampleCurve(seg(0.64, 0.20, 0.71, 0.04), 8),
    ...sampleCurve(seg(0.78, 0.20, 0.71, 0.04), 8),
    // Tower tops (horizontal band)
    ...sampleCurve(seg(0.22, 0.20, 0.36, 0.20), 6),
    ...sampleCurve(seg(0.64, 0.20, 0.78, 0.20), 6),
    // Central nave (shorter, between towers)
    ...sampleCurve(seg(0.36, 0.95, 0.36, 0.42), 17),
    ...sampleCurve(seg(0.64, 0.95, 0.64, 0.42), 17),
    // Nave roof: pointed Gothic arch
    ...sampleCurve(quad(0.36, 0.42, 0.50, 0.26, 0.64, 0.42), 22),
    // Rose window
    ...sampleCurve(arc(0.50, 0.52, 0.10, 0.10, 0, TAU), 22),
    ...sampleCurve(arc(0.50, 0.52, 0.06, 0.06, 0, TAU), 14),
    // Three Gothic portals at base
    ...sampleCurve(arc(0.29, 0.88, 0.07, 0.055, PI, 0), 12),
    ...sampleCurve(arc(0.50, 0.88, 0.07, 0.055, PI, 0), 12),
    ...sampleCurve(arc(0.71, 0.88, 0.07, 0.055, PI, 0), 12),
    // Flying buttresses
    ...sampleCurve(seg(0.36, 0.65, 0.26, 0.80), 8),
    ...sampleCurve(seg(0.64, 0.65, 0.74, 0.80), 8),
    // Ground
    ...sampleCurve(seg(0.10, 0.95, 0.90, 0.95), 24),
  ];
  return resample(scatterAround(raw, 0.005), count);
}

function buildGoldenGate(count: number): ShapePoint[] {
  const raw: ShapePoint[] = [
    // Left tower: two vertical columns
    ...sampleCurve(seg(0.27, 0.90, 0.27, 0.10), 26),
    ...sampleCurve(seg(0.30, 0.90, 0.30, 0.10), 26),
    // Right tower
    ...sampleCurve(seg(0.70, 0.90, 0.70, 0.10), 26),
    ...sampleCurve(seg(0.73, 0.90, 0.73, 0.10), 26),
    // Tower caps
    ...sampleCurve(seg(0.24, 0.10, 0.33, 0.10), 6),
    ...sampleCurve(seg(0.67, 0.10, 0.76, 0.10), 6),
    // Tower cross-bars
    ...sampleCurve(seg(0.24, 0.28, 0.33, 0.28), 6),
    ...sampleCurve(seg(0.67, 0.28, 0.76, 0.28), 6),
    // Main cable: left anchor → left tower top
    ...sampleCurve(quad(0.02, 0.60, 0.14, 0.28, 0.285, 0.12), 22),
    // Main cable: left tower → right tower (the big sag)
    ...sampleCurve(quad(0.285, 0.12, 0.50, 0.72, 0.715, 0.12), 38),
    // Main cable: right tower → right anchor
    ...sampleCurve(quad(0.715, 0.12, 0.86, 0.28, 0.98, 0.60), 22),
    // Deck
    ...sampleCurve(seg(0.01, 0.69, 0.99, 0.69), 42),
    // Vertical suspenders on main span (connecting cable to deck)
    ...Array.from({ length: 16 }, (_, i) => {
      const t = i / 15;
      const x = 0.285 + t * (0.715 - 0.285);
      // Cable height at this x (quadratic approximation)
      const cableY = 0.12 + 4 * (0.72 - 0.12) * t * (1 - t); // parabola peak at t=0.5 → 0.72
      return sampleCurve(seg(x, 0.69, x, cableY), 4);
    }).flat(),
    // Water
    ...sampleCurve(quad(0.0, 0.82, 0.50, 0.78, 1.0, 0.82), 34),
    ...sampleCurve(quad(0.0, 0.87, 0.50, 0.84, 1.0, 0.87), 28),
  ];
  return resample(scatterAround(raw, 0.005), count);
}

function buildBurjKhalifa(count: number): ShapePoint[] {
  // Centered in narrow column: x 0.32..0.68, y 0.02..0.97
  // Stepped/tapered profile
  const raw: ShapePoint[] = [
    // Spire
    ...sampleCurve(seg(0.48, 0.10, 0.50, 0.02), 5),
    ...sampleCurve(seg(0.52, 0.10, 0.50, 0.02), 5),
    // Tier 5 (top)
    ...sampleCurve(seg(0.44, 0.22, 0.44, 0.10), 8),
    ...sampleCurve(seg(0.56, 0.22, 0.56, 0.10), 8),
    ...sampleCurve(seg(0.44, 0.10, 0.56, 0.10), 6),
    // Step out
    ...sampleCurve(seg(0.44, 0.22, 0.40, 0.22), 3),
    ...sampleCurve(seg(0.56, 0.22, 0.60, 0.22), 3),
    // Tier 4
    ...sampleCurve(seg(0.40, 0.37, 0.40, 0.22), 9),
    ...sampleCurve(seg(0.60, 0.37, 0.60, 0.22), 9),
    ...sampleCurve(seg(0.40, 0.22, 0.60, 0.22), 8),
    // Step out
    ...sampleCurve(seg(0.40, 0.37, 0.36, 0.37), 3),
    ...sampleCurve(seg(0.60, 0.37, 0.64, 0.37), 3),
    // Tier 3
    ...sampleCurve(seg(0.36, 0.54, 0.36, 0.37), 11),
    ...sampleCurve(seg(0.64, 0.54, 0.64, 0.37), 11),
    ...sampleCurve(seg(0.36, 0.37, 0.64, 0.37), 11),
    // Step out
    ...sampleCurve(seg(0.36, 0.54, 0.33, 0.54), 2),
    ...sampleCurve(seg(0.64, 0.54, 0.67, 0.54), 2),
    // Tier 2
    ...sampleCurve(seg(0.33, 0.72, 0.33, 0.54), 12),
    ...sampleCurve(seg(0.67, 0.72, 0.67, 0.54), 12),
    ...sampleCurve(seg(0.33, 0.54, 0.67, 0.54), 12),
    // Step out
    ...sampleCurve(seg(0.33, 0.72, 0.30, 0.72), 2),
    ...sampleCurve(seg(0.67, 0.72, 0.70, 0.72), 2),
    // Tier 1 (base)
    ...sampleCurve(seg(0.30, 0.92, 0.30, 0.72), 14),
    ...sampleCurve(seg(0.70, 0.92, 0.70, 0.72), 14),
    ...sampleCurve(seg(0.30, 0.72, 0.70, 0.72), 14),
    // Ground
    ...sampleCurve(seg(0.10, 0.95, 0.90, 0.95), 22),
  ];
  return resample(scatterAround(raw, 0.005), count);
}

function buildSpaceElevator(count: number): ShapePoint[] {
  const raw: ShapePoint[] = [
    // Earth surface (curved arc at bottom)
    ...sampleCurve(arc(0.50, 1.18, 0.60, 0.28, PI * 1.08, PI * 1.92), 34),
    // Tether (thin double line, very vertical)
    ...sampleCurve(seg(0.488, 0.90, 0.488, 0.12), 35),
    ...sampleCurve(seg(0.512, 0.90, 0.512, 0.12), 35),
    // Orbital station ring (outer)
    ...sampleCurve(arc(0.50, 0.10, 0.16, 0.07, 0, TAU), 28),
    // Orbital station ring (inner)
    ...sampleCurve(arc(0.50, 0.10, 0.10, 0.045, 0, TAU), 18),
    // Climber pod (small box riding the tether)
    ...sampleCurve(seg(0.43, 0.54, 0.57, 0.54), 6),
    ...sampleCurve(seg(0.43, 0.64, 0.57, 0.64), 6),
    ...sampleCurve(seg(0.43, 0.54, 0.43, 0.64), 5),
    ...sampleCurve(seg(0.57, 0.54, 0.57, 0.64), 5),
    // Cloud layer
    ...sampleCurve(quad(0.08, 0.78, 0.30, 0.73, 0.50, 0.75), 14),
    ...sampleCurve(quad(0.50, 0.75, 0.70, 0.73, 0.92, 0.78), 14),
    // Stars (scattered points in upper region)
    ...Array.from({ length: 22 }, (_, i) => {
      const a = (i / 22) * TAU + 0.3;
      const r = 0.14 + ((i * 7) % 5) * 0.055;
      return { x: 0.50 + Math.cos(a) * r * 2.2, y: 0.35 + Math.sin(a) * r };
    }),
  ];
  return resample(scatterAround(raw, 0.006), count);
}

// --- Resample: take arbitrary-length point array, return exactly `count` points
// by cycling/subsampling so the particle count is always predictable.
function resample(pts: ShapePoint[], count: number): ShapePoint[] {
  if (pts.length === 0) return [];
  const out: ShapePoint[] = [];
  const ratio = pts.length / count;
  for (let i = 0; i < count; i++) {
    const srcIdx = Math.min(Math.floor(i * ratio), pts.length - 1);
    out.push({ ...pts[srcIdx]! });
  }
  return shuffle(out);
}

// --- Public API ---

export function buildShape(id: ShapeId, canvasW: number, canvasH: number, count = 280): ShapePoint[] {
  const norm = (() => {
    switch (id) {
      case 'sphinx':
        return buildSphinx(count);
      case 'colosseum':
        return buildColosseum(count);
      case 'cathedral':
        return buildCathedral(count);
      case 'goldengate':
        return buildGoldenGate(count);
      case 'burjkhalifa':
        return buildBurjKhalifa(count);
      case 'spaceelevator':
        return buildSpaceElevator(count);
    }
  })();
  // Scale normalized 0..1 coords to canvas pixel space
  return scale(norm, canvasW, canvasH);
}
