'use client';

import bamlMark from '@boundaryml/brand/logo/svg/baml-logomark-white.svg';
import bamlWhite from '@boundaryml/brand/logo/svg/baml-primary-white.svg';
import Image from 'next/image';
import {
  type CSSProperties,
  Fragment,
  type RefObject,
  useEffect,
  useRef,
  useState,
} from 'react';
import { Card, Frame } from './kit';
import { MarkTile } from './mark';
import { MARKS, type Mark } from './marks';

// The v4 hero diagram: where each toolchain reaches, from build to production. The languages fill
// Build and Test, then hand everything past the deploy line to separate products, drawn as grids of
// their logos, which appear one stage at a time. Deploy is a labelled line in a gap of its own, so the
// cells on either side of it never touch. BAML's row is the one thing that crosses it: a single bar that
// narrows to a thick neck over the line, and a line runs from its end back to Build. It is drawn with
// the parts from /diagram-styles, so the style button can show it in any of that board's styles.

const BEFORE = ['Build', 'Test'];
const AFTER = ['Trace', 'Dashboards', 'Alerts', 'Feature flags'];
const STAGES = [...BEFORE, ...AFTER];

export const PRODUCTS: Mark[][] = [
  [MARKS.datadog, MARKS.honeycomb, MARKS.langsmith, MARKS.logfire],
  [MARKS.grafana, MARKS.datadog, MARKS.metabase],
  [MARKS.sentry, MARKS.pagerduty, MARKS.datadog],
  [MARKS.launchdarkly, MARKS.statsig, MARKS.posthog],
];

type Test = { command: string } | { marks: Mark[] } | { own: true };

// Each language is named, with no logo, since C's logo is just the letter again. `short` is the name
// on a phone, where the column is narrow. `year` is when the language first appeared, shown faintly
// under its name.
const LANGUAGES: {
  build: string;
  name: string;
  short: string;
  test: Test;
  year: number;
}[] = [
  { build: 'cc', name: 'C', short: 'C', test: { own: true }, year: 1972 },
  {
    build: 'tsc',
    name: 'TypeScript',
    short: 'TS',
    test: { marks: [MARKS.jest, MARKS.vitest, MARKS.mocha] },
    year: 2012,
  },
  {
    build: 'go build',
    name: 'Go',
    short: 'Go',
    test: { command: 'go test' },
    year: 2009,
  },
];

const BAML = [
  'baml check',
  'baml test',
  'baml query',
  'baml dashboard',
  'baml alerts',
  'baml flags',
];

const FILL_MS = 6000;
const HOLD_MS = 3400;
const LAP_MS = FILL_MS + HOLD_MS;
// Once BAML's bar is full, a band of light crosses it, starting just after the fill lands.
const SWEEP_AT = FILL_MS + 150;
const SWEEP_MS = 1400;

const MUTED = 'rgb(from var(--ds-ink) r g b / 0.55)';
const GRID =
  'grid grid-cols-[52px_repeat(2,minmax(0,1fr))_14px_repeat(4,minmax(0,1fr))] md:grid-cols-[132px_repeat(2,minmax(0,1fr))_48px_repeat(4,minmax(0,1fr))] gap-x-1 md:gap-x-2 items-center';
// Row and padding sizes are variables set by .tracker in home.css, which shrinks them on short screens.
const ROW = 'h-(--tracker-row)';

/** `n` held between 0 and 1. */
const clamp = (n: number) => Math.min(1, Math.max(0, n));
/** Quadratic ease in and out over 0 to 1. */
const ease = (t: number) => (t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2);

/** The grid column a stage sits in: past the name column, and past the deploy line after Test. */
const column = (stage: number) => stage + 2 + (stage >= BEFORE.length ? 1 : 0);

// BAML's lamb steps out of the logo at the start of each lap and runs along the top of the bar with
// the fill, hopping over the deploy line. It is drawn as tall as the room above the bar allows,
// between `RUNNER_MIN` and `RUNNER_MAX` pixels, and from 768px up the logo's own lamb dims while it is
// away.
const RUNNER_MIN = 18;
const RUNNER_MAX = 24;
/** The lamb's width for a height of `h`, in the logomark's proportions. */
const lambWidth = (h: number) => (h * 356) / 422;

// The logomark is pixel art on a 32 by 38 grid, and the runner is a set of sprite frames drawn from
// this map of it. Each letter marks a part that the frames pose on its own: `#` the body, `h` the
// head, `e` the ear, `t` the tail, `a` and `b` the near and far hind legs, and `f` the folded
// forelegs. The running frames leave out the logomark's legs and draw longer ones. A capital letter
// is a cell of the parent part that the child also draws, so a joint stays filled as the child
// turns.
const LAMB = [
  '........................hhh.....',
  '......................hhhhhhh...',
  '................eeeee.Ehhhhhhh..',
  '................eeeeeeEhhhhhhh..',
  '..................eeeeEhhhhhhhhh',
  '.....................hhhhhhhhhhh',
  '.....................hhhhhhhhhhh',
  '.....................hhhhhhhhh..',
  '....................HHHHHHHH....',
  '....................########....',
  '..................##########....',
  '................############....',
  '...............#############....',
  '.tt.........################....',
  '.ttt.....###################....',
  '.ttttt.#####################....',
  '.TTTTT########################..',
  '..#############################.',
  '..##########################FFF.',
  '.....######################.fff.',
  '.....####################..fff..',
  '.....###################.fffff..',
  '.....################...fffff...',
  '.....##############....ffff.....',
  '.....############......ff.......',
  '....###########.................',
  '..############..................',
  '..###########...................',
  '.###########....................',
  '.AA#BBB##.......................',
  '.aa.bbb.........................',
  '.aa.bb..........................',
  'aa.bbb..........................',
  'aa.bbb..........................',
  '..bbb...........................',
  '.bbb............................',
  'bbb.............................',
  'bb..............................',
];

type Point = [number, number];

/** One pose of the lamb, from which a sprite frame is drawn. Angles are in degrees, clockwise:
 *  positive tips the body and head nose down and raises the ear and tail. The body turns about its
 *  middle, then moves `dy` cells down. `sx` and `sy` squash or stretch it about its feet.
 *
 *  A pose either keeps the logomark's own legs, turned about their joints by `turns` (positive swings
 *  a leg back), or draws all four legs fresh from the hip or shoulder to a hoof at `hooves`, in the
 *  canvas's cells. A hoof at `GROUND` is planted on the bar. `dust` is a few cells of dust. */
type Rig = {
  dust?: Point[];
  dy: number;
  ear: number;
  head: number;
  hooves?: Record<Leg, Point>;
  pitch: number;
  sx: number;
  sy: number;
  tail: number;
  turns?: Record<Leg, number>;
};

/** The legs: `a` and `b` the near and far hind legs, `f` and `g` the near and far forelegs. */
type Leg = 'a' | 'b' | 'f' | 'g';

const LOGO_POSE: Rig = {
  dy: 0,
  ear: 0,
  head: 0,
  pitch: 0,
  sx: 1,
  sy: 1,
  tail: 0,
};

/** A 2D affine transform, as SVG writes one: x' = a x + c y + e and y' = b x + d y + f. */
type Affine = [number, number, number, number, number, number];

const multiply = (m: Affine, n: Affine): Affine => [
  m[0] * n[0] + m[2] * n[1],
  m[1] * n[0] + m[3] * n[1],
  m[0] * n[2] + m[2] * n[3],
  m[1] * n[2] + m[3] * n[3],
  m[0] * n[4] + m[2] * n[5] + m[4],
  m[1] * n[4] + m[3] * n[5] + m[5],
];

const apply = (m: Affine, [x, y]: Point): Point => [
  m[0] * x + m[2] * y + m[4],
  m[1] * x + m[3] * y + m[5],
];

/** A turn of `deg` degrees clockwise about the cell point (`x`, `y`). */
function turn(deg: number, x: number, y: number): Affine {
  const r = (deg * Math.PI) / 180;
  const cos = Math.cos(r);
  const sin = Math.sin(r);
  return [cos, sin, -sin, cos, x - cos * x + sin * y, y - sin * x - cos * y];
}

function invert([a, b, c, d, e, f]: Affine): Affine {
  const det = a * d - b * c;
  return [
    d / det,
    -b / det,
    -c / det,
    a / det,
    (c * f - d * e) / det,
    (b * e - a * f) / det,
  ];
}

/** Where a pose puts the body: squashed about the feet, turned about its middle, and lowered. */
const bodyOf = (rig: Rig) =>
  multiply(
    [1, 0, 0, 1, 0, rig.dy],
    multiply(turn(rig.pitch, 16, 20), [
      rig.sx,
      0,
      0,
      rig.sy,
      16 - 16 * rig.sx,
      38 - 38 * rig.sy,
    ]),
  );

// A running lamb's back is level, so its frames turn the logomark's leaping body this many degrees
// nose down and lower it `RUN_DROP` cells, and lift its head back up by `HEAD_UP`.
const LEVEL = 18;
const RUN_DROP = 4.5;
const HEAD_UP = -10;
// The canvas row that is the top of the bar. A planted hoof's lowest cell sits on it.
const GROUND = 37.5;
// Where each drawn leg meets the body, in the logomark's cells.
const JOINTS: Record<Leg, Point> = {
  a: [3, 26.5],
  b: [6.5, 26],
  f: [23, 21],
  g: [21.5, 21.5],
};
// The two segments of a drawn leg, in cells, and their half widths.
const UPPER = 5.8;
const LOWER = 6.6;
// Where each joint sits when the body is level, so hooves can be placed relative to it.
const LEVEL_BODY = bodyOf({ ...LOGO_POSE, dy: RUN_DROP, pitch: LEVEL });
const AT = Object.fromEntries(
  (Object.keys(JOINTS) as Leg[]).map((leg) => [
    leg,
    apply(LEVEL_BODY, JOINTS[leg])[0],
  ]),
) as Record<Leg, number>;

/** A running pose: the body level and lowered, tipped `pitch` more and `dy` lower, with its legs
 *  drawn to `hooves`, each given as a step from under its joint and a height. */
function run(rig: Partial<Rig> & { steps: Record<Leg, Point> }): Rig {
  const { steps, ...rest } = rig;
  return {
    ...LOGO_POSE,
    ...rest,
    dy: RUN_DROP + (rig.dy ?? 0),
    head: HEAD_UP + (rig.head ?? 0),
    hooves: {
      a: [AT.a + steps.a[0], steps.a[1]],
      b: [AT.b + steps.b[0], steps.b[1]],
      f: [AT.f + steps.f[0], steps.f[1]],
      g: [AT.g + steps.g[0], steps.g[1]],
    },
    pitch: LEVEL + (rig.pitch ?? 0),
  };
}

/** A leg's hoof `phase` of the way through a stride: planted for the 40% of the stride from `start`,
 *  sliding back from `front` to `back` cells under its joint as the lamb runs over it, then lifted
 *  `lift` cells and swung forward again. */
function stepAt(
  phase: number,
  start: number,
  front: number,
  back: number,
  lift: number,
): Point {
  const q = (((phase - start) % 1) + 1) % 1;
  const STANCE = 0.4;
  if (q < STANCE) return [front - ((front - back) * q) / STANCE, GROUND];
  const w = (q - STANCE) / (1 - STANCE);
  const swing = w * w * (3 - 2 * w);
  return [back + (front - back) * swing, GROUND - lift * Math.sin(Math.PI * w)];
}

/** The lamb `phase` of the way through a gallop stride. At phase 0 the near foreleg plants ahead of
 *  the shoulder, with the far one just behind it, and the body pitches nose down onto them. As the
 *  forelegs lift, the hind legs plant and push off, nose up and stretched, and from phase 0.83 all
 *  four are off the bar. The head nods against the body a little behind it, and the ear and tail
 *  trail further behind. */
function gallop(phase: number): Rig {
  const wave = (lag: number) => Math.cos(2 * Math.PI * (phase - lag));
  return run({
    dy: -wave(0.9),
    ear: -14 - 14 * wave(0.6),
    head: -5 * wave(0.2),
    pitch: 4 * wave(0.15),
    steps: {
      a: stepAt(phase, 0.38, 3, -1.5, 3),
      b: stepAt(phase, 0.43, 3, -1.5, 3),
      f: stepAt(phase, 0, 3.5, -3, 5),
      g: stepAt(phase, 0.07, 4, -3, 5),
    },
    sx: 1 - 0.04 * wave(0.9),
    sy: 1 + 0.05 * wave(0.9),
    tail: -14 - 16 * wave(0.65),
  });
}

const GALLOP_FRAMES = 8;

/** Four legs planted a little apart, standing. */
const STANDING: Record<Leg, Point> = {
  a: [0.5, GROUND],
  b: [2.5, GROUND],
  f: [1, GROUND],
  g: [-1.5, GROUND],
};

// The frames, by name. The gallop's frames come first, then the poses for the pop out of the logo,
// the hop over the deploy line, the skid to a stop, and the rest at the end of the bar.
const POSES: Rig[] = [
  ...Array.from({ length: GALLOP_FRAMES }, (_, i) => gallop(i / GALLOP_FRAMES)),
  // The logomark itself, which the runner starts as.
  LOGO_POSE,
  // Crouched on the logo, in the logomark's pose, to spring out of it.
  {
    dy: 0,
    ear: 8,
    head: 10,
    pitch: 3,
    sx: 1.08,
    sy: 0.86,
    tail: 6,
    turns: { a: -28, b: -28, f: 12, g: 12 },
  },
  // Springing out of the logo: hind legs pushing a little back, forelegs forward, nose up and
  // stretched.
  {
    dy: 0,
    ear: -26,
    head: -8,
    pitch: -14,
    sx: 0.92,
    sy: 1.14,
    tail: -30,
    turns: { a: 6, b: 6, f: -35, g: -30 },
  },
  // Crouched on the bar to jump: legs gathered under it and bent, body low, head down.
  run({
    dy: 2.5,
    ear: 6,
    head: 6,
    steps: {
      a: [2, GROUND],
      b: [3, GROUND],
      f: [-1, GROUND],
      g: [-2, GROUND],
    },
    sx: 1.06,
    sy: 0.9,
    tail: 4,
  }),
  // Taking off: hind hooves still pushing on the bar far back, forelegs lifted forward, nose up and
  // stretched.
  run({
    dy: -1,
    ear: -26,
    head: -6,
    pitch: -16,
    steps: {
      a: [-0.5, GROUND],
      b: [0.5, GROUND],
      f: [6, 31],
      g: [5, 32],
    },
    sx: 0.93,
    sy: 1.1,
    tail: -30,
  }),
  // In the air: all four legs tucked up, ear and tail streaming back.
  run({
    dy: -1,
    ear: -36,
    head: -4,
    pitch: -4,
    steps: {
      a: [3, 32],
      b: [4, 31.5],
      f: [1, 31],
      g: [0, 32],
    },
    tail: -40,
  }),
  // Coming down: nose down, forelegs reaching for the bar, hind legs still tucked.
  run({
    ear: -30,
    head: 6,
    pitch: 10,
    steps: {
      a: [2, 32.5],
      b: [3, 32],
      f: [5, 36.5],
      g: [3.5, 36],
    },
    sx: 0.96,
    sy: 1.06,
    tail: -34,
  }),
  // Landed: squashed on four planted hooves, with the ear and tail thrown forward.
  run({
    dy: 2.5,
    ear: 18,
    head: 10,
    pitch: 5,
    steps: {
      a: [1, GROUND],
      b: [2, GROUND],
      f: [2, GROUND],
      g: [3.5, GROUND],
    },
    sx: 1.1,
    sy: 0.84,
    tail: 20,
  }),
  // Skidding: leaning back with the forelegs braced out ahead and the hind legs tucked under it,
  // all four hooves sliding on the bar, the ear and tail flung forward, and dust kicked up behind
  // the hooves. The two frames swap the dust.
  ...[0, 1].map((puff) =>
    run({
      dust:
        puff === 0
          ? [
              [AT.a - 6, 37],
              [AT.a - 7, 35.5],
              [AT.a - 9, 36.5],
              [AT.f + 9, 37],
            ]
          : [
              [AT.a - 8, 36],
              [AT.a - 10, 34.5],
              [AT.a - 11, 36.5],
              [AT.f + 10, 35.5],
              [AT.f + 11, 37],
            ],
      dy: 1.5,
      ear: 22,
      head: -6,
      pitch: -10,
      steps: {
        a: [2, GROUND],
        b: [3, GROUND],
        f: [6.5, GROUND],
        g: [5, GROUND],
      },
      sx: 1.04,
      sy: 0.95,
      tail: 18,
    }),
  ),
  // Settling after the skid: rocking forward onto the forelegs as the last of the dust drifts on
  // ahead of it.
  run({
    dust: [
      [AT.f + 12, 36],
      [AT.f + 14, 34.5],
    ],
    ear: 14,
    head: 4,
    pitch: 6,
    steps: STANDING,
    tail: 12,
  }),
  // Standing on the bar, raised on nearly straight legs.
  run({ dy: -1, steps: STANDING }),
  // A breath in, standing.
  run({ dy: -0.9, ear: -4, head: -2, steps: STANDING, sy: 1.03 }),
  // A flick of the ear, standing.
  run({ dy: -1, ear: 28, steps: STANDING }),
];
const LOGO = GALLOP_FRAMES;
const POP_CROUCH = GALLOP_FRAMES + 1;
const POP_STRETCH = GALLOP_FRAMES + 2;
const CROUCH = GALLOP_FRAMES + 3;
const STRETCH = GALLOP_FRAMES + 4;
const AIR = GALLOP_FRAMES + 5;
const DESCEND = GALLOP_FRAMES + 6;
const LAND = GALLOP_FRAMES + 7;
const SKID = GALLOP_FRAMES + 8;
const SETTLE = GALLOP_FRAMES + 10;
const STAND = GALLOP_FRAMES + 11;
const INHALE = GALLOP_FRAMES + 12;
const FLICK = GALLOP_FRAMES + 13;

// A sprite frame is drawn on a canvas of cells larger than the logomark, so a stretched or tilted
// pose has room. The logomark's own 32 by 38 cells start `PAD_X` and `PAD_Y` cells in.
const PAD_X = 10;
const PAD_Y = 8;
const CANVAS_W = 32 + 2 * PAD_X;
const CANVAS_H = 38 + PAD_Y + 6;

/** How far the point (`x`, `y`) is from the segment from `p` to `q`. */
function toSegment(x: number, y: number, [px, py]: Point, [qx, qy]: Point) {
  const dx = qx - px;
  const dy = qy - py;
  const k = clamp(((x - px) * dx + (y - py) * dy) / (dx * dx + dy * dy || 1));
  return Math.hypot(x - px - k * dx, y - py - k * dy);
}

/** A drawn leg from its joint to its hoof, bent at one knee. The knee points forward, so a bent hind
 *  leg folds under the body rather than out behind it. */
function limb(hip: Point, hoof: Point): [Point, Point, Point] {
  const dx = hoof[0] - hip[0];
  const dy = hoof[1] - hip[1];
  const d = Math.min(Math.hypot(dx, dy), UPPER + LOWER - 0.01);
  const ux = dx / Math.hypot(dx, dy);
  const uy = dy / Math.hypot(dx, dy);
  const bend = Math.acos(
    clamp((UPPER * UPPER + d * d - LOWER * LOWER) / (2 * UPPER * d)),
  );
  const [nx, ny] = [uy, -ux];
  // A knee never dips into the bar, even on a leg folded under a low body.
  const knee: Point = [
    hip[0] + UPPER * (Math.cos(bend) * ux + Math.sin(bend) * nx),
    Math.min(
      GROUND - 1.5,
      hip[1] + UPPER * (Math.cos(bend) * uy + Math.sin(bend) * ny),
    ),
  ];
  return [hip, knee, [hip[0] + d * ux, hip[1] + d * uy]];
}

/** One SVG path of the filled cells of `grid`, a rectangle per run along a row. */
function cellsPath(grid: boolean[][]) {
  let d = '';
  grid.forEach((row, j) => {
    let i = 0;
    while (i < row.length) {
      if (!row[i]) {
        i++;
        continue;
      }
      const start = i;
      while (row[i]) i++;
      d += `M${start - PAD_X} ${j - PAD_Y}h${i - start}v1h${start - i}z`;
    }
  });
  return d;
}

/** Draws one sprite frame from a pose: a cell of the canvas is filled when most of it falls on the
 *  posed logomark, or when it lies on a drawn leg. Lone cells are then cleared and pinholes filled,
 *  as a pixel artist would. */
function drawFrame(rig: Rig) {
  const body = bodyOf(rig);
  const head = multiply(body, turn(rig.head, 24, 8.5));
  const own = rig.turns;
  const joints: [string, Affine][] = [
    ['#ABFHT', body],
    ['hEH', head],
    ['eE', multiply(head, turn(rig.ear, 22, 3.5))],
    ['tT', multiply(body, turn(rig.tail, 3.5, 16))],
  ];
  if (!rig.hooves) {
    // The logomark's own legs move with the body but are not squashed or stretched with it, so they
    // keep their length.
    const plain = bodyOf({ ...rig, sx: 1, sy: 1 });
    const leg = (deg: number, x: number, y: number): Affine => {
      const [px, py] = apply(body, [x, y]);
      const [qx, qy] = apply(plain, [x, y]);
      return multiply(
        [1, 0, 0, 1, px - qx, py - qy],
        multiply(plain, turn(deg, x, y)),
      );
    };
    joints.push(
      ['aA', leg(own?.a ?? 0, 2, 30)],
      ['bB', leg(own?.b ?? 0, 5.5, 30)],
      ['fF', leg(own?.f ?? 0, 29.5, 19)],
      ['fF', leg(own?.g ?? 0, 29.5, 19)],
    );
  }
  const parts = joints.map(([keep, m]): [string, Affine] => [keep, invert(m)]);
  const limbs = rig.hooves
    ? (Object.keys(JOINTS) as Leg[]).map((leg) =>
        limb(apply(body, JOINTS[leg]), rig.hooves?.[leg] ?? [0, 0]),
      )
    : [];
  const onLamb = (x: number, y: number) =>
    parts.some(([keep, m]) => {
      const [col, row] = apply(m, [x, y]).map(Math.floor);
      return keep.includes(LAMB[row]?.[col] ?? '.');
    });
  const onLeg = (x: number, y: number) =>
    limbs.some(
      ([hip, knee, hoof]) =>
        toSegment(x, y, hip, knee) <= 1.15 ||
        toSegment(x, y, knee, hoof) <= 0.8,
    );
  const grid = Array.from({ length: CANVAS_H }, (_, j) =>
    Array.from({ length: CANVAS_W }, (_, i) => {
      let hits = 0;
      for (const [dx, dy] of [
        [0.25, 0.25],
        [0.75, 0.25],
        [0.25, 0.75],
        [0.75, 0.75],
      ]) {
        if (onLamb(i - PAD_X + dx, j - PAD_Y + dy)) hits++;
      }
      return hits >= 2;
    }),
  );
  const at = (i: number, j: number) => grid[j]?.[i] ?? false;
  // The turned logomark is tidied: lone cells are cleared and pinholes filled. The logomark itself
  // is left exactly as it is, and the drawn legs are added after, so a hoof's tip is never cleared.
  const clean = grid.map((row, j) =>
    row.map((on, i) => {
      if (rig === LOGO_POSE) return on;
      const touching = [
        at(i - 1, j),
        at(i + 1, j),
        at(i, j - 1),
        at(i, j + 1),
      ].filter(Boolean).length;
      const corners = [
        at(i - 1, j - 1),
        at(i + 1, j - 1),
        at(i - 1, j + 1),
        at(i + 1, j + 1),
      ].filter(Boolean).length;
      if (on) return touching + corners > 1;
      return touching >= 3;
    }),
  );
  const legged = clean.map((row, j) =>
    row.map((on, i) => on || onLeg(i - PAD_X + 0.5, j - PAD_Y + 0.5)),
  );
  // Where two legs cross they can close in a single empty cell, which is filled.
  const cells = legged.map((row, j) =>
    row.map(
      (on, i) =>
        on ||
        (rig !== LOGO_POSE &&
          [
            legged[j][i - 1],
            legged[j][i + 1],
            legged[j - 1]?.[i],
            legged[j + 1]?.[i],
          ].every(Boolean)),
    ),
  );
  const dust = Array.from({ length: CANVAS_H }, (_, j) =>
    Array.from({ length: CANVAS_W }, (_, i) =>
      (rig.dust ?? []).some(
        ([x, y]) => Math.floor(x) === i - PAD_X && Math.floor(y) === j - PAD_Y,
      ),
    ),
  );
  return { d: cellsPath(cells), dust: cellsPath(dust) };
}

let frames: { d: string; dust: string }[] | null = null;
/** The runner's sprite frames, drawn the first time the runner shows. */
const lambFrames = () => {
  frames ??= POSES.map(drawFrame);
  return frames;
};

// The height of the arc the runner springs out of the logo on.
const POP_LIFT = 12;
// The hop starts and ends this many pixels of bar before and after the neck, and rises `HOP_LIFT`.
const HOP_LEAD = 40;
const HOP_LIFT = 18;
// The length of bar the runner skids along as it stops at the end.
const SKID_PX = 40;
// Late in the hold, the runner fades out at the end of the bar as the logo's lamb comes back.
const FADE_AT = LAP_MS - 700;
const FADE_MS = 400;

/** Milliseconds into the lap, every frame while `frame` is on screen, and whether the lap is held
 *  still. Off screen the lap pauses where it is, so the diagram costs nothing while it's scrolled
 *  away. Under reduced motion it holds at the end of the fill. */
function useLap(frame: RefObject<HTMLElement | null>) {
  const [t, setT] = useState(0);
  const [still, setStill] = useState(false);
  useEffect(() => {
    const el = frame.current;
    if (!el) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setT(FILL_MS + 1);
      setStill(true);
      return;
    }
    let id = 0;
    let last = 0;
    let lap = 0;
    const tick = (now: number) => {
      if (last) lap = (lap + Math.min(now - last, 100)) % LAP_MS;
      last = now;
      setT(lap);
      id = requestAnimationFrame(tick);
    };
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting && !id) {
        last = 0;
        id = requestAnimationFrame(tick);
      } else if (!entry.isIntersecting && id) {
        cancelAnimationFrame(id);
        id = 0;
      }
    });
    observer.observe(el);
    return () => {
      cancelAnimationFrame(id);
      observer.disconnect();
    };
  }, [frame]);
  return { still, t };
}

/** Whether the screen is at least 640px wide, or null before the page has loaded, when both of the
 *  diagram's layouts render and CSS shows the right one. */
function useWide() {
  const [wide, setWide] = useState<boolean | null>(null);
  useEffect(() => {
    const query = window.matchMedia('(min-width: 40rem)');
    const sync = () => setWide(query.matches);
    sync();
    query.addEventListener('change', sync);
    return () => query.removeEventListener('change', sync);
  }, []);
  return wide;
}

/** A card that fills from the left as the lap reaches its stage, or from the top in the phone's
 *  vertical diagram. */
function Fill({ fill, vertical }: { fill: number; vertical?: boolean }) {
  return (
    <span
      aria-hidden
      className={`absolute inset-0 ${vertical ? 'origin-top' : 'origin-left'}`}
      style={{
        background: 'rgb(from var(--ds-ink) r g b / 0.12)',
        transform: vertical ? `scaleY(${fill})` : `scaleX(${fill})`,
      }}
    />
  );
}

/** The outline of BAML's bar, `w` long and `h` thick, pinched between `a` and `b` (the deploy gap) to
 *  a neck 24% of its thickness. Its outer corners take the style's card radius `r`. It runs left to
 *  right, or top to bottom when `vertical`: the same outline with x and y swapped, which mirrors it, so
 *  its corner arcs turn the other way. */
function barPath(
  w: number,
  h: number,
  a: number,
  b: number,
  r: number,
  vertical = false,
) {
  const t = h * 0.38;
  const s = Math.min(12, (b - a) / 4);
  /** The point `x` along the bar and `y` across it, in the drawing's coordinates. */
  const pt = (x: number, y: number) => (vertical ? `${y} ${x}` : `${x} ${y}`);
  const arc = (x: number, y: number) =>
    `A ${r} ${r} 0 0 ${vertical ? 0 : 1} ${pt(x, y)}`;
  return [
    `M ${pt(r, 0)}`,
    `L ${pt(a - s, 0)}`,
    `C ${pt(a, 0)} ${pt(a, t)} ${pt(a + s, t)}`,
    `L ${pt(b - s, t)}`,
    `C ${pt(b, t)} ${pt(b, 0)} ${pt(b + s, 0)}`,
    `L ${pt(w - r, 0)}`,
    arc(w, r),
    `L ${pt(w, h - r)}`,
    arc(w - r, h),
    `L ${pt(b + s, h)}`,
    `C ${pt(b, h)} ${pt(b, h - t)} ${pt(b - s, h - t)}`,
    `L ${pt(a + s, h - t)}`,
    `C ${pt(a, h - t)} ${pt(a, h)} ${pt(a - s, h)}`,
    `L ${pt(r, h)}`,
    arc(0, h - r),
    `L ${pt(0, r)}`,
    arc(r, 0),
    'Z',
  ].join(' ');
}

type Shape = {
  a: number;
  b: number;
  d: string;
  h: number;
  /** The logo's lamb, relative to the bar: the middle of its feet and its height. */
  home: { h: number; x: number; y: number } | null;
  len: number;
  /** The running lamb's height. */
  size: number;
  w: number;
};

/** How far along the bar the fill has reached when the lap has filled `p` stages, in pixels. Each
 *  side of the deploy line moves at its own stages' pace, and the neck fills quickly in between. */
function reach(p: number, { a, b, len }: Shape) {
  const n = BEFORE.length;
  if (p <= n) return (p / n) * a;
  if (p <= n + 0.2) return a + ((p - n) / 0.2) * (b - a);
  return b + ((p - n - 0.2) / (AFTER.length - 0.2)) * (len - b);
}

/** The inverse of `reach`: how many stages the lap has filled when the fill is `x` pixels along. */
function stagesAt(x: number, { a, b, len }: Shape) {
  const n = BEFORE.length;
  if (x <= a) return (x / a) * n;
  if (x <= b) return n + ((x - a) / (b - a)) * 0.2;
  return n + 0.2 + ((x - b) / (len - b)) * (AFTER.length - 0.2);
}

/** Quadratic ease out over 0 to 1. */
const easeOut = (t: number) => 1 - (1 - t) ** 2;

/** How many stages the lap has filled `t` milliseconds in. */
const stagesBy = (t: number) => ease(clamp(t / FILL_MS)) * STAGES.length;

/** The inverse of `stagesBy`: when in the lap `p` stages are filled. */
function timeAt(p: number) {
  const e = clamp(p / STAGES.length);
  return (
    (e < 0.5 ? Math.sqrt(e / 2) : 1 - Math.sqrt(2 * (1 - e)) / 2) * FILL_MS
  );
}

/** Where BAML's running lamb is `t` milliseconds into the lap: the middle of its feet relative to the
 *  top left of the bar, which sprite frame shows, its scale, and its opacity. */
function runnerPose(t: number, shape: Shape) {
  const { a, b, home, len, size } = shape;
  const half = lambWidth(size) / 2;
  const lo = half + 4;
  const hi = len - half - 6;
  /** Where the fill would carry it at time `at`: its nose at the fill's front, kept clear of the
   *  bar's rounded ends. */
  const follow = (at: number) =>
    Math.min(hi, Math.max(lo, reach(stagesBy(at), shape) - half));
  // Three quarters of the way through the fill, as the fill starts to slow, the lamb keeps its pace
  // and runs on ahead of it, then brakes and skids the last `SKID_PX` to a stop at the end of the
  // bar, slowing evenly.
  const lead = FILL_MS * 0.75;
  const xLead = follow(lead);
  const vLead = Math.max(0.01, (follow(lead + 8) - follow(lead - 8)) / 16);
  const xBrake = Math.max(xLead, hi - SKID_PX);
  const brake = lead + (xBrake - xLead) / vLead;
  const stop = brake + (2 * (hi - xBrake)) / vLead;
  /** Where it is along the bar at time `at`. */
  const along = (at: number) => {
    if (at <= lead) return follow(at);
    if (at <= brake) return xLead + vLead * (at - lead);
    if (at >= stop) return hi;
    const s = at - brake;
    return xBrake + vLead * s - (vLead * s * s) / (2 * (stop - brake));
  };
  // How fast it is running, from 0 standing to 1 at a full gallop.
  const speed = clamp((along(t + 8) - along(t - 8)) / 16 / (len / 4000));
  // A stride covers a tenth of the bar, and the gallop's frames advance with the distance run, so
  // its legs stop when it stops.
  const strideLen = len / 10;
  // The hop: a crouch before takeoff, the jump from `HOP_LEAD` pixels before the neck to as far past
  // it, and a squash on landing. Strides are counted from the takeoff and from the landing.
  const t0 = timeAt(stagesAt(a - HOP_LEAD, shape));
  const t1 = timeAt(stagesAt(b + HOP_LEAD, shape));
  const x0 = along(t0);
  const x1 = along(t1);

  let x = along(t);
  let y = 0;
  let frame = STAND;
  let scale = 1;

  if (t < 660) {
    // The pop out of the logo: a crouch on the logo's lamb at its size, a stretch as it springs,
    // legs tucked in the air, forelegs reaching down, and a squash as it lands on the bar.
    frame =
      t < 40
        ? LOGO
        : t < 170
          ? POP_CROUCH
          : t < 250
            ? POP_STRETCH
            : t < 470
              ? AIR
              : t < 560
                ? DESCEND
                : LAND;
    if (home) {
      const k = clamp((t - 170) / 390);
      const out = easeOut(k);
      x = home.x + (x - home.x) * out;
      y = home.y * (1 - out) - 4 * POP_LIFT * k * (1 - k);
      scale = home.h / size + (1 - home.h / size) * out;
    }
  } else if (t >= t0 - 110 && t < t1 + 110) {
    if (t < t0) {
      frame = CROUCH;
    } else if (t < t1) {
      const s = (t - t0) / (t1 - t0);
      frame = s < 0.18 ? STRETCH : s < 0.62 ? AIR : DESCEND;
      x = x0 + s * (x1 - x0);
      y = -4 * HOP_LIFT * s * (1 - s);
    } else {
      frame = LAND;
    }
  } else if (t >= brake && t < stop) {
    // The skid, kicking up dust from one frame to the next.
    frame = SKID + (Math.floor((t - brake) / 70) % 2);
  } else if (t >= stop) {
    // It rocks forward as it stops, then stands breathing, with a flick of the ear now and then,
    // until it fades.
    const idle = t - stop - 220;
    const breath = (((idle % 1800) + 1800) % 1800) / 1800;
    frame =
      idle < 0
        ? SETTLE
        : (idle >= 1100 && idle < 1240) || (idle >= 2300 && idle < 2440)
          ? FLICK
          : breath > 0.4 && breath < 0.85
            ? INHALE
            : STAND;
  } else if (speed > 0.06) {
    // The gallop's bob is drawn into its frames, so a planted hoof stays on the bar.
    const phase = (x - (t < t0 ? x0 : x1)) / strideLen;
    frame = Math.floor((phase - Math.floor(phase)) * GALLOP_FRAMES);
  }

  const opacity = 1 - clamp((t - FADE_AT) / FADE_MS);
  return { frame, opacity, scale, x, y };
}

/** How brightly the logo's own lamb shows. Its copy covers it exactly at the start of the lap, so it
 *  hides as the copy crouches, then shows dimmed while the copy is out on the bar, and is whole again
 *  before the next lap starts. */
function homeOpacity(t: number) {
  if (t <= 0) return 1;
  if (t < FADE_AT) return 0.3 * clamp((t - 250) / 250);
  return 0.3 + 0.7 * clamp((t - FADE_AT) / FADE_MS);
}

/** BAML's bar, from Build to Feature flags in one piece. It is measured against its grid so its neck
 *  sits exactly over the deploy gap: across the columns, or down the rows when `vertical` (the phone's
 *  diagram, where `style` places it in its column). Until it is measured it draws as a plain card. It is
 *  keyed by the diagram style, so a style with a different card radius measures it again. Given the
 *  logo's lamb as `home` and the lap's time `t`, the wide bar also runs a copy of the lamb along its
 *  top, outside the bar's clip so none of it is cut off. */
function BamlBar({
  full,
  home,
  p,
  style,
  sweep,
  t,
  vertical = false,
}: {
  full: boolean;
  home?: RefObject<HTMLElement | null>;
  p: number;
  style?: CSSProperties;
  sweep: number;
  t?: number;
  vertical?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [shape, setShape] = useState<Shape | null>(null);

  useEffect(() => {
    const bar = ref.current;
    const grid = bar?.parentElement;
    if (!bar || !grid) return;
    const measure = () => {
      const css = getComputedStyle(grid);
      const tracks = (vertical ? css.gridTemplateRows : css.gridTemplateColumns)
        .split(' ')
        .map(Number.parseFloat);
      const gap = Number.parseFloat(vertical ? css.rowGap : css.columnGap) || 0;
      const w = bar.clientWidth;
      const h = bar.clientHeight;
      const [len, thick] = vertical ? [h, w] : [w, h];
      const r = Math.min(
        thick / 2,
        Number.parseFloat(
          getComputedStyle(bar).getPropertyValue('--ds-card-radius'),
        ) || 0,
      );
      // The bar starts at the first stage's track; the neck runs from the end of Test to the start
      // of Trace, gaps included.
      const a = tracks[1] + gap + tracks[2];
      const b = a + gap + tracks[3] + gap;
      // The logo's lamb, measured from the bar so the runner can start exactly on top of it.
      const lamb = home?.current?.getBoundingClientRect();
      const box = bar.getBoundingClientRect();
      // The runner is as tall as the room between the bar and the language rows above it, within
      // limits. The caption between them shows only once the runner has passed under it.
      const rows = grid.parentElement?.firstElementChild;
      const room = rows
        ? box.top - rows.getBoundingClientRect().bottom
        : RUNNER_MAX;
      setShape({
        a,
        b,
        d: barPath(len, thick, a, b, r, vertical),
        h,
        home: lamb?.height
          ? {
              h: lamb.height,
              x: lamb.left + lamb.width / 2 - box.left,
              y: lamb.bottom - box.top,
            }
          : null,
        len,
        size: Math.min(RUNNER_MAX, Math.max(RUNNER_MIN, room)),
        w,
      });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(grid);
    return () => observer.disconnect();
  }, [home, vertical]);

  // Where the fill has reached, as a share of the bar's length.
  const at = shape?.len
    ? clamp(reach(p, shape) / shape.len)
    : p / STAGES.length;
  const pose =
    shape?.len && t !== undefined && !vertical ? runnerPose(t, shape) : null;
  // The size of one cell of the logomark's grid in the runner.
  const cell = (shape?.size ?? RUNNER_MIN) / 38;

  const inner = (
    <>
      <span
        aria-hidden
        className={`absolute inset-0 ${vertical ? 'origin-top' : 'origin-left'}`}
        style={{
          // The fill brightens a step when the whole bar is full.
          background: `rgb(from var(--ds-accent) r g b / ${full ? 0.42 : 0.3})`,
          transform: vertical ? `scaleY(${at})` : `scaleX(${at})`,
          transition: 'background-color 400ms',
        }}
      />
      <span
        aria-hidden
        className="absolute inset-0 pointer-events-none"
        style={{
          opacity: sweep > 0 && sweep < 1 ? 1 : 0,
          transform: vertical
            ? `translateY(${-18 + sweep * 118}%)`
            : `translateX(${-18 + sweep * 118}%)`,
        }}
      >
        <span
          className={`absolute left-0 top-0 ${vertical ? 'inset-x-0 h-[18%]' : 'inset-y-0 w-[18%]'}`}
          style={{
            background: `linear-gradient(${vertical ? 180 : 90}deg, transparent, rgb(from var(--ds-accent-ink) r g b / 0.4), transparent)`,
            filter: 'blur(4px)',
          }}
        />
      </span>
    </>
  );

  return (
    <div
      className={vertical ? 'relative self-stretch' : `relative ${ROW}`}
      ref={ref}
      style={style ?? { gridColumn: '2 / -1' }}
    >
      {shape?.len ? (
        <>
          <div
            className="absolute inset-0 overflow-hidden"
            // No backdrop blur: Chrome blurs the whole box behind a path clip, which showed as square
            // corners. The frame under the bar is blurred already.
            style={{
              background: 'var(--ds-card)',
              clipPath: `path('${shape.d}')`,
            }}
          >
            {inner}
          </div>
          <svg
            aria-hidden
            className="absolute inset-0 overflow-visible pointer-events-none"
            height={shape.h}
            viewBox={`0 0 ${shape.w} ${shape.h}`}
            width={shape.w}
          >
            <title>BAML's bar outline</title>
            <path
              d={shape.d}
              fill="none"
              stroke="var(--ds-lit-border)"
              strokeWidth={1}
            />
          </svg>
          {pose && (
            // Drawn in cells of the logomark's grid, `cell` pixels each, and placed by its feet: the
            // middle of the logomark's bottom edge, which is also the origin of its scale.
            <svg
              aria-hidden
              className="absolute left-0 top-0 overflow-visible pointer-events-none"
              fill="white"
              height={CANVAS_H * cell}
              shapeRendering="crispEdges"
              style={{
                opacity: pose.opacity,
                transform: `translate(${pose.x - (16 + PAD_X) * cell}px, ${pose.y - (38 + PAD_Y) * cell}px) scale(${pose.scale})`,
                transformOrigin: `${(16 + PAD_X) * cell}px ${(38 + PAD_Y) * cell}px`,
              }}
              viewBox={`${-PAD_X} ${-PAD_Y} ${CANVAS_W} ${CANVAS_H}`}
              width={CANVAS_W * cell}
            >
              <title>BAML's lamb</title>
              <path d={lambFrames()[pose.frame].d} />
              <path d={lambFrames()[pose.frame].dust} fillOpacity={0.55} />
            </svg>
          )}
        </>
      ) : (
        <Card className="absolute inset-0 overflow-hidden" lit>
          {inner}
        </Card>
      )}
    </div>
  );
}

/** A build or test command in a stage's cell. From 640px it shows in small type, wrapping onto two
 *  lines where a cell is narrow; from 768px it sits on one line. Phones have no room for it. `lit`
 *  gives it the accent. */
function Command({ children, lit }: { children: string; lit?: boolean }) {
  return (
    <span
      className={`relative hidden sm:flex h-full items-center justify-center px-1 font-mono text-[10px] leading-[1.2] text-center md:text-[12px] lg:text-[13px] md:whitespace-nowrap md:truncate ${lit ? 'text-(--ds-accent-ink)' : 'text-(--ds-ink)/80'}`}
    >
      {children}
    </span>
  );
}

/** The lap's state at one moment, shared by the wide diagram and the phone's. */
type Lap = {
  back: boolean;
  deployed: boolean;
  full: boolean;
  handed: boolean;
  lineColor: string;
  p: number;
  shown: (i: number) => boolean;
  style: string;
  sweep: number;
};

// The phone's diagram turns the wide one on its side: stages run down the rows, the languages and
// BAML across the columns. The columns are the stage names, the three languages, BAML, and a narrow
// one for the line back from feature flags to build. The rows are the column heads, Build and Test,
// the deploy line, then the four stages past it; `column` numbers them the same way.
const VGRID =
  'grid grid-cols-[68px_repeat(3,minmax(0,1fr))_92px_12px] grid-rows-[32px_repeat(2,32px)_20px_repeat(4,32px)] gap-1.5';

/** The hero diagram on a phone, with BAML's bar filling from the top down. */
function Vertical({
  back,
  deployed,
  full,
  handed,
  lineColor,
  p,
  shown,
  style,
  sweep,
}: Lap) {
  const deployRow = column(BEFORE.length) - 1;
  return (
    // Its cells are about as wide as they are tall, so it takes a smaller corner radius than the
    // style's, which would round them into pills.
    <div className="sm:hidden [--ds-card-radius:5px]">
      <div className={`${VGRID} relative`}>
        {LANGUAGES.map((lang, i) => (
          <span
            className="self-center text-center text-[13px] text-(--ds-ink)/80"
            key={lang.name}
            style={{ gridColumn: i + 2, gridRow: 1 }}
          >
            {lang.short}
          </span>
        ))}
        <span
          className="self-center justify-self-center"
          style={{ gridColumn: 5, gridRow: 1 }}
        >
          <Image
            alt="BAML"
            className="h-8 w-auto"
            height={32}
            src={bamlMark}
            width={27}
          />
        </span>

        {STAGES.map((s, i) => (
          <span
            className="self-center text-[12px] leading-tight transition-opacity duration-500"
            key={s}
            style={{
              color: MUTED,
              gridColumn: 1,
              gridRow: column(i),
              opacity: i < BEFORE.length || shown(i - BEFORE.length) ? 1 : 0,
            }}
          >
            {s}
          </span>
        ))}

        {/* The deploy line, across the languages and through BAML's neck. */}
        <span
          className="self-center font-mono text-[11px] transition-colors duration-500"
          style={{
            color: deployed ? 'var(--ds-accent-ink)' : MUTED,
            gridColumn: 1,
            gridRow: deployRow,
          }}
        >
          deploy
        </span>
        <span
          aria-hidden
          className="self-center h-0 border-t-2 border-dashed transition-[border-color] duration-500"
          style={{
            borderColor: lineColor,
            gridColumn: '2 / 6',
            gridRow: deployRow,
          }}
        />

        {LANGUAGES.map((lang, i) => (
          <Fragment key={lang.name}>
            <Card
              className="relative overflow-hidden"
              style={{ gridColumn: i + 2, gridRow: column(0) }}
            >
              <Fill fill={clamp(p)} vertical />
            </Card>
            {'command' in lang.test ? (
              <Card
                className="relative overflow-hidden"
                style={{ gridColumn: i + 2, gridRow: column(1) }}
              >
                <Fill fill={clamp(p - 1)} vertical />
              </Card>
            ) : (
              // No test tool of its own: a dashed cell, like the products past the line.
              <span
                className="rounded-(--ds-card-radius) border border-dashed border-(--ds-line)"
                style={{ gridColumn: i + 2, gridRow: column(1) }}
              />
            )}
          </Fragment>
        ))}

        {PRODUCTS.map((marks, i) => (
          <div
            className="flex items-center justify-center gap-1.5 rounded-(--ds-card-radius) border border-dashed border-(--ds-line) text-(--ds-ink)/75 transition-opacity duration-500"
            key={AFTER[i]}
            style={{
              gridColumn: '2 / 5',
              gridRow: column(BEFORE.length + i),
              opacity: shown(i) ? 1 : 0,
            }}
          >
            {marks.map((m) => (
              <MarkTile key={m.name} mark={m} size={16} />
            ))}
          </div>
        ))}

        <BamlBar
          full={full}
          key={style}
          p={p}
          style={{ gridColumn: 5, gridRow: `${column(0)} / -1` }}
          sweep={sweep}
          vertical
        />
        {BAML.map((command, i) => (
          <span
            className="self-center text-center font-mono text-[10px] text-(--ds-accent-ink) whitespace-nowrap pointer-events-none transition-opacity duration-300"
            key={command}
            style={{
              gridColumn: 5,
              gridRow: column(i),
              opacity: clamp(p - i) > 0.4 ? 1 : 0.35,
            }}
          >
            {command}
          </span>
        ))}

        {/* BAML's line runs back up from feature flags to build. */}
        <span
          aria-hidden
          className="relative"
          style={{ gridColumn: 6, gridRow: `${column(0)} / -1` }}
        >
          <span
            className="absolute inset-y-[18px] -left-1.5 right-0.5 rounded-r-[10px] border border-l-0 border-(--ds-accent)"
            style={{
              clipPath: back
                ? 'inset(-2px -2px -2px 0)'
                : 'inset(100% -2px -2px 0)',
              opacity: back ? 1 : 0,
              transition: back
                ? 'clip-path 1400ms cubic-bezier(0.65,0,0.35,1)'
                : 'none',
            }}
          />
          <span
            className="absolute right-0.5 top-1/2 translate-x-1/2 -translate-y-1/2 rotate-180 py-2 bg-(--ds-ground) text-[11px] text-(--ds-accent-ink) [writing-mode:vertical-rl] transition-opacity duration-500"
            style={{ opacity: back ? 1 : 0 }}
          >
            Improve
          </span>
        </span>
      </div>
      <p
        className="mt-3 text-[11.5px] transition-opacity duration-500"
        style={{ color: MUTED, opacity: handed ? 1 : 0 }}
      >
        Each logo past deploy is a separate product with its own SDK.
      </p>
    </div>
  );
}

/** The v4 hero diagram in the given /diagram-styles style, played as a looping lap from build to
 *  production. */
export default function Tracker({ style }: { style: string }) {
  const root = useRef<HTMLDivElement>(null);
  const lamb = useRef<HTMLImageElement>(null);
  const { still, t } = useLap(root);
  // Only the layout on screen renders once the width is known, so each frame updates one, not two.
  const wide = useWide();
  // How many stages the lap has filled, in stage widths.
  const p = ease(clamp(t / FILL_MS)) * STAGES.length;
  const deployed = p >= BEFORE.length;
  const handed = p >= BEFORE.length + 0.3;
  const back = t >= FILL_MS + 300;
  const full = t >= FILL_MS;
  // Where the light is across the bar, from 0 to 1. Under reduced motion it never starts.
  const sweep = t > SWEEP_AT ? ease(clamp((t - SWEEP_AT) / SWEEP_MS)) : 0;
  // Past the line, each stage's products appear as the lap reaches that stage, one after another.
  const shown = (i: number) => p >= BEFORE.length + i + 0.15;
  const lineColor = deployed
    ? 'var(--ds-accent)'
    : 'rgb(from var(--ds-ink) r g b / 0.45)';

  return (
    <div
      className={`tracker ds-theme ds-theme-${style} rounded-[28px] p-2 sm:p-3`}
      ref={root}
    >
      <Frame label="Where each toolchain reaches. C builds with cc and leaves testing to you. TypeScript builds with tsc and tests with Jest, Vitest, or Mocha. Go builds and tests with go build and go test. Past deploy, each of them hands tracing, dashboards, alerts, and feature flags to 11 separate products. BAML covers every stage itself, and a line runs from production back to build, labelled Improve.">
        {/* Cards take a smaller corner radius than the style's, so they read as boxes, not pills. */}
        <div className="relative p-(--tracker-pad) [--ds-card-radius:8px]">
          {wide !== true && (
            <Vertical
              back={back}
              deployed={deployed}
              full={full}
              handed={handed}
              lineColor={lineColor}
              p={p}
              shown={shown}
              style={style}
              sweep={sweep}
            />
          )}
          {wide !== false && (
            <div className="relative hidden sm:block">
              {/* The deploy line, drawn once from under its label to the bottom of BAML's row, in the gap
                column, so it runs through every row without touching the cells on either side. */}
              <div
                aria-hidden
                className={`${GRID} absolute inset-x-0 top-(--tracker-head) bottom-[calc(var(--tracker-ret)+6px)] md:bottom-[calc(var(--tracker-ret)+8px)] pointer-events-none`}
              >
                <span
                  className="justify-self-center self-stretch w-0 border-l-2 border-dashed transition-[border-color] duration-500"
                  style={{
                    borderColor: lineColor,
                    gridColumn: column(BEFORE.length) - 1,
                    gridRow: 1,
                  }}
                />
              </div>

              <div className={`${GRID} h-(--tracker-head)`}>
                <span
                  className="justify-self-center font-mono text-[11px] md:text-[13px] whitespace-nowrap transition-colors duration-500"
                  style={{
                    color: deployed ? 'var(--ds-accent-ink)' : MUTED,
                    gridColumn: column(BEFORE.length) - 1,
                  }}
                >
                  deploy
                </span>
                <span
                  className="hidden md:block justify-self-center text-[15px] font-medium text-(--ds-ink)"
                  style={{
                    gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                  }}
                >
                  Production
                </span>
              </div>

              {/* Column labels. On a phone, the two sides of the line are labelled instead. */}
              <div
                className={`${GRID} mb-2 md:mb-3 text-[12.5px] md:text-[14px]`}
                style={{ color: MUTED }}
              >
                {STAGES.map((s, i) => (
                  <span
                    className="hidden md:block text-center truncate transition-opacity duration-500"
                    key={s}
                    style={{
                      gridColumn: column(i),
                      opacity:
                        i < BEFORE.length || shown(i - BEFORE.length) ? 1 : 0,
                    }}
                  >
                    {s}
                  </span>
                ))}
                <span
                  className="md:hidden"
                  style={{ gridColumn: '2 / span 2' }}
                >
                  Before
                </span>
                <span
                  className="md:hidden"
                  style={{ gridColumn: '5 / span 4' }}
                >
                  In production
                </span>
              </div>

              <div className="relative flex flex-col gap-1.5 md:gap-2">
                {/* One grid for the language rows, so each product column can span all three. */}
                <div className={`${GRID} gap-y-1.5 md:gap-y-2`}>
                  {LANGUAGES.map((lang, i) => (
                    <Fragment key={lang.name}>
                      <div
                        className="flex items-center gap-2 min-w-0 text-(--ds-ink)/80"
                        style={{ gridColumn: 1, gridRow: i + 1 }}
                      >
                        <span className="md:hidden text-[14px]">
                          {lang.short}
                        </span>
                        <span className="hidden md:flex flex-col min-w-0 leading-tight">
                          <span className="text-[15px] truncate">
                            {lang.name}
                          </span>
                          <span className="text-[12px] text-faint">
                            {lang.year}
                          </span>
                        </span>
                      </div>

                      <Card
                        className={`relative ${ROW} overflow-hidden`}
                        style={{ gridColumn: column(0), gridRow: i + 1 }}
                      >
                        <Fill fill={clamp(p)} />
                        <Command>{lang.build}</Command>
                      </Card>

                      {'command' in lang.test ? (
                        <Card
                          className={`relative ${ROW} overflow-hidden`}
                          style={{ gridColumn: column(1), gridRow: i + 1 }}
                        >
                          <Fill fill={clamp(p - 1)} />
                          <Command>{lang.test.command}</Command>
                        </Card>
                      ) : (
                        // No test tool of its own: a dashed cell, like the products past the line. On a
                        // phone it stays, empty, so every language row has a build and a test cell.
                        <div
                          className={`${ROW} flex items-center justify-center gap-2.5 rounded-(--ds-card-radius) border border-dashed border-(--ds-line) text-[12.5px] text-center text-(--ds-ink)/80 *:max-md:hidden`}
                          style={{
                            gridColumn: column(1),
                            gridRow: i + 1,
                            ...('own' in lang.test ? { color: MUTED } : {}),
                          }}
                        >
                          {'own' in lang.test ? (
                            <span>Bring your own</span>
                          ) : (
                            lang.test.marks.map((m) => (
                              <MarkTile key={m.name} mark={m} size={18} />
                            ))
                          )}
                        </div>
                      )}
                    </Fragment>
                  ))}

                  {/* Past the line, each stage is a set of separate products, the same for every language. */}
                  {PRODUCTS.map((marks, i) => (
                    <div
                      className="self-stretch hidden md:flex flex-wrap content-center items-center justify-center gap-3 px-2 rounded-(--ds-card-radius) border border-dashed border-(--ds-line) transition-[opacity,translate] duration-500 text-(--ds-ink)/75"
                      key={AFTER[i]}
                      style={{
                        gridColumn: column(BEFORE.length + i),
                        gridRow: `1 / span ${LANGUAGES.length}`,
                        opacity: shown(i) ? 1 : 0,
                        translate: shown(i) ? '0 0' : '0 6px',
                      }}
                    >
                      {marks.map((m) => (
                        <MarkTile key={m.name} mark={m} size={22} />
                      ))}
                    </div>
                  ))}
                  <p
                    className="self-stretch md:hidden flex items-center justify-center rounded-(--ds-card-radius) border border-dashed border-(--ds-line) px-1 text-center text-[11.5px] transition-opacity duration-500"
                    style={{
                      color: MUTED,
                      gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                      gridRow: `1 / span ${LANGUAGES.length}`,
                      opacity: handed ? 1 : 0,
                    }}
                  >
                    11 separate products
                  </p>
                </div>

                <div className={`${GRID} tracker-caption`}>
                  <p
                    className="hidden md:block text-center text-[12.5px] md:text-[13px] pt-0.5 transition-opacity duration-500"
                    style={{
                      color: MUTED,
                      gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                      opacity: shown(AFTER.length - 1) ? 1 : 0,
                    }}
                  >
                    Separate products, each with its own SDK
                  </p>
                </div>

                <div className={`${GRID} relative mt-1 md:mt-2`}>
                  {/* BAML's logo at the brand's 32px minimum: the logomark fits the phone's narrow name
                  column, the primary logo the wide one. Every diagram style is dark, so both are white.
                  The lamb is drawn on its own, over the primary logo with the logo's lamb clipped
                  away, so it can dim while its copy runs along the bar. It sits where the primary
                  logo draws it: 68.01% across and 31.7px tall. It dims only from 768px up: below
                  that the logomark is the whole logo, so it stays whole. */}
                  <div className="flex items-center min-w-0">
                    <span
                      aria-label="BAML"
                      className="relative flex"
                      role="img"
                    >
                      <Image
                        alt=""
                        className="hidden md:block h-8 w-auto [clip-path:inset(0_33%_0_0)]"
                        height={32}
                        src={bamlWhite}
                        width={83}
                      />
                      <Image
                        alt=""
                        className="h-8 w-auto md:absolute md:top-0 md:left-[68.01%] md:h-[31.7px] md:opacity-(--lamb-home)"
                        height={32}
                        ref={lamb}
                        src={bamlMark}
                        style={
                          {
                            '--lamb-home': still ? 1 : homeOpacity(t),
                          } as CSSProperties
                        }
                        width={27}
                      />
                    </span>
                  </div>
                  <BamlBar
                    full={full}
                    home={lamb}
                    key={style}
                    p={p}
                    sweep={sweep}
                    t={still ? undefined : t}
                  />
                  {/* The commands sit over the block, in the same columns as the stages above them. */}
                  <div
                    className={`${GRID} absolute inset-0 pointer-events-none`}
                  >
                    {BAML.map((command, i) => (
                      <span
                        className={`${ROW} min-w-0 transition-opacity duration-300`}
                        key={command}
                        style={{
                          gridColumn: column(i),
                          gridRow: 1,
                          opacity: clamp(p - i) > 0.4 ? 1 : 0.35,
                        }}
                      >
                        <Command lit>{command}</Command>
                      </span>
                    ))}
                  </div>
                </div>

                {/* BAML's row runs back from feature flags to build. */}
                <div aria-hidden className={`${GRID} h-(--tracker-ret)`}>
                  <div
                    className="relative self-start h-[70%] mx-[4%]"
                    style={{ gridColumn: '2 / -1' }}
                  >
                    <span
                      className="absolute inset-0 rounded-b-[12px] border border-t-0 border-(--ds-accent)"
                      style={{
                        clipPath: back
                          ? 'inset(0 -2px -2px 0)'
                          : 'inset(0 -2px -2px 100%)',
                        // A fully clipped border still left a sliver of its right edge under the bar.
                        opacity: back ? 1 : 0,
                        transition: back
                          ? 'clip-path 1400ms cubic-bezier(0.65,0,0.35,1)'
                          : 'none',
                      }}
                    />
                    <span
                      className="absolute left-1/2 bottom-0 translate-y-1/2 -translate-x-1/2 px-3 bg-(--ds-ground) text-[12.5px] md:text-[14px] text-(--ds-accent-ink) transition-opacity duration-500"
                      style={{ opacity: back ? 1 : 0 }}
                    >
                      Improve
                    </span>
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>
      </Frame>
    </div>
  );
}
