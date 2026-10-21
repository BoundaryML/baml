// Camera for the canvas views.
//
// Moving between two far-apart files uses van Wijk and Nuij's "smooth and
// efficient zooming and panning" path: zoom out while panning, then zoom back
// in, so the trip covers the least perceived distance. You see where the
// destination sits relative to where you were, which is what makes a quick
// jump readable instead of dizzying. (This is the same path d3-zoom uses.)

export interface View {
  /** World point at the center of the viewport. */
  cx: number;
  cy: number;
  /** World units per screen pixel is 1/k. */
  k: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Lower rho zooms out less on the way; 1.2 keeps a trip between neighbors nearly flat. */
const RHO = 1.2;

export function zoomPath(
  from: View,
  to: View,
  viewportWidth: number,
): (t: number) => View {
  const ux0 = from.cx;
  const uy0 = from.cy;
  const w0 = viewportWidth / from.k;
  const ux1 = to.cx;
  const uy1 = to.cy;
  const w1 = viewportWidth / to.k;
  const dx = ux1 - ux0;
  const dy = uy1 - uy0;
  const d2 = dx * dx + dy * dy;
  const rho2 = RHO * RHO;
  const rho4 = rho2 * rho2;

  if (d2 < 1e-6) {
    const S = Math.log(w1 / w0) / RHO;
    return (t) => {
      const w = w0 * Math.exp(RHO * t * S);
      return { cx: ux0 + t * dx, cy: uy0 + t * dy, k: viewportWidth / w };
    };
  }
  const d1 = Math.sqrt(d2);
  const b0 = (w1 * w1 - w0 * w0 + rho4 * d2) / (2 * w0 * rho2 * d1);
  const b1 = (w1 * w1 - w0 * w0 - rho4 * d2) / (2 * w1 * rho2 * d1);
  const r0 = Math.log(Math.sqrt(b0 * b0 + 1) - b0);
  const r1 = Math.log(Math.sqrt(b1 * b1 + 1) - b1);
  const S = (r1 - r0) / RHO;
  return (t) => {
    const s = t * S;
    const coshr0 = Math.cosh(r0);
    const u =
      (w0 / (rho2 * d1)) * (coshr0 * Math.tanh(RHO * s + r0) - Math.sinh(r0));
    const w = (w0 * coshr0) / Math.cosh(RHO * s + r0);
    return { cx: ux0 + u * dx, cy: uy0 + u * dy, k: viewportWidth / w };
  };
}

export const easeInOutCubic = (t: number) =>
  t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2;

/** The view that fits `rect` in a viewport, with `margin` screen pixels around it. */
export function fitView(
  rect: Rect,
  vw: number,
  vh: number,
  margin: number,
  maxK: number,
  minK = 0.05,
): View {
  const k = Math.max(
    minK,
    Math.min(maxK, (vw - margin * 2) / rect.w, (vh - margin * 2) / rect.h),
  );
  return { cx: rect.x + rect.w / 2, cy: rect.y + rect.h / 2, k };
}

export function unionRects(rects: Rect[]): Rect | null {
  if (!rects.length) return null;
  let x0 = Number.POSITIVE_INFINITY;
  let y0 = Number.POSITIVE_INFINITY;
  let x1 = Number.NEGATIVE_INFINITY;
  let y1 = Number.NEGATIVE_INFINITY;
  for (const r of rects) {
    x0 = Math.min(x0, r.x);
    y0 = Math.min(y0, r.y);
    x1 = Math.max(x1, r.x + r.w);
    y1 = Math.max(y1, r.y + r.h);
  }
  return { h: y1 - y0, w: x1 - x0, x: x0, y: y0 };
}

/**
 * Owns a view and animates it. Frames are applied through `apply`, outside
 * React, so a flight never re-renders the scene.
 */
export class Camera {
  view: View = { cx: 0, cy: 0, k: 1 };
  vw = 800;
  vh = 600;
  private frame = 0;
  private flight: {
    path: (t: number) => View;
    start: number;
    duration: number;
    done?: () => void;
  } | null = null;

  constructor(private apply: (view: View) => void) {}

  get flying() {
    return this.flight !== null;
  }

  set(view: View) {
    this.stop();
    this.view = view;
    this.apply(view);
  }

  flyTo(target: View, duration: number, done?: () => void) {
    this.stop();
    const path = zoomPath(this.view, target, this.vw);
    this.flight = {
      done,
      duration: Math.max(1, duration),
      path,
      start: performance.now(),
    };
    const tick = () => {
      const f = this.flight;
      if (!f) return;
      const t = Math.min(1, (performance.now() - f.start) / f.duration);
      this.view = f.path(easeInOutCubic(t));
      this.apply(this.view);
      if (t < 1) this.frame = requestAnimationFrame(tick);
      else {
        this.flight = null;
        f.done?.();
      }
    };
    this.frame = requestAnimationFrame(tick);
  }

  stop() {
    cancelAnimationFrame(this.frame);
    this.flight = null;
  }

  /** Pan by a screen-space delta. */
  panBy(dx: number, dy: number) {
    this.stop();
    this.view = {
      ...this.view,
      cx: this.view.cx - dx / this.view.k,
      cy: this.view.cy - dy / this.view.k,
    };
    this.apply(this.view);
  }

  /** Zoom by `factor` keeping the world point under screen (sx, sy) fixed. */
  zoomAt(factor: number, sx: number, sy: number, minK: number, maxK: number) {
    this.stop();
    const { cx, cy, k } = this.view;
    const nk = Math.max(minK, Math.min(maxK, k * factor));
    const wx = cx + (sx - this.vw / 2) / k;
    const wy = cy + (sy - this.vh / 2) / k;
    this.view = {
      cx: wx - (sx - this.vw / 2) / nk,
      cy: wy - (sy - this.vh / 2) / nk,
      k: nk,
    };
    this.apply(this.view);
  }

  /** CSS transform placing world coordinates on screen. */
  transform(view: View = this.view): string {
    const tx = this.vw / 2 - view.cx * view.k;
    const ty = this.vh / 2 - view.cy * view.k;
    return `translate(${tx}px, ${ty}px) scale(${view.k})`;
  }
}
