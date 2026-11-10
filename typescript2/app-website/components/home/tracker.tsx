'use client';

import bamlMark from '@boundaryml/brand/logo/svg/baml-logomark-white.svg';
import bamlWhite from '@boundaryml/brand/logo/svg/baml-primary-white.svg';
import { RotateCcw } from 'lucide-react';
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

// The v4 hero diagram: where each toolchain reaches, from build to production. Four columns on each
// side of the line: the language itself, then Build, Test, and Publish, which the languages fill; past
// the deploy line they hand everything to separate products, drawn as grids of
// their logos, which appear one stage at a time. Deploy is a labelled line in a gap of its own, so the
// cells on either side of it never touch. BAML's row is the one thing that crosses it: a single bar that
// narrows to a thick neck over the line, and a line runs from its end back to Build. It is drawn with
// the parts from /diagram-styles, so the style button can show it in any of that board's styles.

const BEFORE = ['Build', 'Test', 'Release'];
const AFTER = ['Trace', 'Dashboards', 'Alerts', 'More'];
const STAGES = [...BEFORE, ...AFTER];
// Shorter stage names for the phone's narrow name column.
const SHORT: Record<string, string> = { Dashboards: 'Metrics' };

// Past the line every tool is there once, in its stage's column, and each language takes its own
// journey through them: one tool per stage, drawn as a line as the lap crosses production. `ROUTES`
// is each language's pick in each stage, as an index into that stage's `TOOLS`.
const TOOLS: Mark[][] = [
  [MARKS.datadog, MARKS.honeycomb, MARKS.langsmith, MARKS.logfire],
  [MARKS.grafana, MARKS.metabase, MARKS.posthog],
  [MARKS.sentry, MARKS.pagerduty],
  [MARKS.launchdarkly, MARKS.statsig, MARKS.temporal, MARKS.cloudflare],
];
// The ways the production side can be drawn, compared on /tracker-lab. `web`: every tool once, in
// columns of different lengths, with curving lines. `transit`: every stage has a slot per language
// row, tools sit in those slots, and each line runs level out of its own row, jogging only where it
// picks a tool in another row.
export const PRODUCTIONS = [
  'web',
  'transit',
  'tangle',
  'fill',
  'reveal',
  'shuffle',
] as const;
export type Production = (typeof PRODUCTIONS)[number];

// The transit layout's slots: one per language row in each stage, some left empty.
const SLOTS: (Mark | null)[][] = [
  [MARKS.datadog, MARKS.honeycomb, MARKS.langsmith, MARKS.logfire],
  [MARKS.grafana, MARKS.metabase, MARKS.posthog, null],
  [MARKS.sentry, null, MARKS.pagerduty, null],
  [MARKS.launchdarkly, MARKS.statsig, MARKS.temporal, MARKS.cloudflare],
];
// Each language's slot in each stage: C runs straight, the others jog.
const LINES: number[][] = [
  [0, 0, 0, 0],
  [2, 1, 0, 1],
  [1, 0, 2, 2],
  [3, 2, 2, 3],
];

// A route through the transit slots: the language row it leaves from and its slot in each stage.
type Route = { from: number; picks: number[] };
const ONE_EACH: Route[] = LINES.map((picks, from) => ({ from, picks }));
// The tangle: three routes out of every row, so the many combinations show, not one each.
const TANGLE: Route[] = [
  [0, [0, 0, 0, 0]],
  [0, [1, 1, 2, 1]],
  [0, [0, 2, 0, 3]],
  [1, [2, 1, 0, 1]],
  [1, [1, 0, 2, 0]],
  [1, [3, 2, 2, 2]],
  [2, [1, 0, 2, 2]],
  [2, [2, 2, 0, 3]],
  [2, [0, 1, 2, 0]],
  [3, [3, 2, 2, 3]],
  [3, [2, 1, 0, 2]],
  [3, [3, 0, 0, 1]],
].map(([from, picks]) => ({ from: from as number, picks: picks as number[] }));

// The fill take: each language row's picks in each stage, popped into a cell of empty squares.
const PICKS: Mark[][][] = [
  [[MARKS.datadog], [MARKS.grafana], [MARKS.pagerduty], [MARKS.launchdarkly]],
  [
    [MARKS.honeycomb, MARKS.langsmith],
    [MARKS.metabase],
    [MARKS.sentry],
    [MARKS.statsig, MARKS.temporal],
  ],
  [
    [MARKS.datadog, MARKS.logfire],
    [MARKS.grafana, MARKS.datadog],
    [MARKS.pagerduty],
    [MARKS.posthog],
  ],
  [
    [MARKS.honeycomb],
    [MARKS.metabase],
    [MARKS.sentry, MARKS.datadog],
    [MARKS.cloudflare],
  ],
];
const SQUARES = 3;
// The shuffle take: once every product is revealed, it cycles through the languages, each beat
// lighting one project's stack and dimming the rest of the pool.
const SHUFFLE_BEAT_MS = 1500;

/** The shuffle's pick out of `n` options for slot `slot` on beat `beat`: scrambled, but the same
 *  every time for the same beat, so a render never flickers. */
function pickOf(beat: number, slot: number, n: number) {
  const h = Math.imul(beat * 31 + slot * 17 + 7, 2654435761) >>> 0;
  return (h >>> 8) % n;
}

// The shuffle's highlighting stays quiet. Every tool rests muted, from the start, and only a pick
// comes up to full strength and grows a touch; the rows not in focus fade a little.
const UNPICKED = 0.45;
const PICK_GROW = 'scale(1.12)';
const UNFOCUSED = 0.8;

// The reveal take: the original columns of products, the boxes there from the start.
const ZONED: Mark[][][] = [
  [
    [MARKS.datadog, MARKS.honeycomb],
    [MARKS.langsmith, MARKS.logfire],
  ],
  [[MARKS.grafana, MARKS.datadog, MARKS.metabase]],
  [[MARKS.sentry, MARKS.pagerduty, MARKS.datadog]],
  [
    [MARKS.launchdarkly, MARKS.statsig, MARKS.posthog],
    [MARKS.temporal, MARKS.cloudflare],
  ],
];

// Where a stage splits into more than one box, what a box is, labelled in its corner. A box that is
// just its stage, like Trace's first, needs none.
const ZONE_NAMES: (string | null)[][] = [
  [null, 'AI traces'],
  [null],
  [null],
  ['Feature flags', 'Durable execution'],
];

/** How far the lap is through past-the-line stage `i`, 0 to 1. */
const stageAt = (p: number, i: number) => clamp(p - BEFORE.length - i);

const ROUTES: number[][] = [
  // C: Datadog, Grafana, PagerDuty, LaunchDarkly
  [0, 0, 1, 0],
  // TypeScript: LangSmith, Metabase, Sentry, Statsig
  [2, 1, 0, 1],
  // Go: Honeycomb, Grafana, PagerDuty, Temporal
  [1, 0, 1, 2],
  // Rust: Logfire, PostHog, Sentry, Cloudflare
  [3, 2, 0, 3],
];
// Every product past the line, counted once.
const PRODUCT_COUNT = new Set(TOOLS.flat().map((m) => m.name)).size;

// A step a language covers: a command it ships, the separate tools people pick between, or nothing
// of its own.
type Step = { command: string } | { marks: Mark[] } | { own: true };

// Each language, with how far its own toolchain reaches: `unified` steps from Build on are one tool,
// drawn as one block with the language, labelled `tool`. The rest are separate cells: a command, the
// tools people pick between, or an empty dashed cell where there is nothing of its own. `steps` are
// Build, Test, and Publish.
const LANGUAGES: {
  logo: Mark;
  name: string;
  steps: [Step, Step, Step];
  tool?: string;
  unified: number;
}[] = [
  {
    logo: MARKS.c,
    name: 'C',
    steps: [{ command: 'cc' }, { own: true }, { own: true }],
    tool: 'cc',
    unified: 1,
  },
  {
    logo: MARKS.typescript,
    name: 'TypeScript',
    steps: [
      { marks: [MARKS.vite, MARKS.webpack, MARKS.esbuild] },
      { marks: [MARKS.jest, MARKS.vitest, MARKS.mocha] },
      { marks: [MARKS.npm, MARKS.pnpm, MARKS.yarn] },
    ],
    unified: 0,
  },
  {
    logo: MARKS.go,
    name: 'Go',
    steps: [{ command: 'go build' }, { command: 'go test' }, { own: true }],
    tool: 'go',
    unified: 2,
  },
  {
    logo: MARKS.rust,
    name: 'Rust',
    steps: [
      { command: 'cargo build' },
      { command: 'cargo test' },
      { command: 'cargo publish' },
    ],
    tool: 'cargo',
    unified: 3,
  },
];

// The phone's columns: the stage names, one per language, BAML, and the line back to build.
const BAML_COL = LANGUAGES.length + 2;

const BAML = [
  'baml check',
  'baml test',
  'baml pack',
  'baml query',
  'baml dashboard',
  'baml alerts',
  'and more',
];

// The lap: the claim that no stack has gone past deploy sits over the diagram from the start, with
// the languages' bars already full. The products past the line pop in, then "Until now.", and only
// then does BAML's row pop in and fill.
// Kept for the runner, which no longer runs: how long the old fill took.
const FILL_MS = 5000;
const PROD_AT = 500;
const UNTIL_AT = PROD_AT + 1600;
const BAML_AT = UNTIL_AT + 700;
const BAML_MS = 1800;
const FULL_AT = BAML_AT + BAML_MS;
// It plays once. Once the Improve line has drawn, the story is done and a Replay button shows.
const DONE_AT = FULL_AT + 1700;
// Kept for the runner, which no longer runs: the length of the old looping lap.
const LAP_MS = FULL_AT + 3400;
// From then on a glimmer keeps circling BAML's loop: across the bar, then back along the Improve
// line to build, a short rest, and around again.
// The cycle's timing is in home.css, in the `tracker-glint-*` keyframes.

const MUTED = 'rgb(from var(--ds-ink) r g b / 0.55)';

/** BAML's row pops in once "Until now." has landed: a fade and a short rise, opacity and transform only. */
function popIn(on: boolean): CSSProperties {
  return {
    opacity: on ? 1 : 0,
    transform: on ? 'translate3d(0, 0, 0)' : 'translate3d(0, 10px, 0)',
    transition: on
      ? 'opacity 500ms ease-out, transform 500ms cubic-bezier(0.2, 0.8, 0.2, 1)'
      : 'none',
  };
}
const GRID =
  'grid grid-cols-[52px_repeat(3,minmax(0,1fr))_14px_repeat(4,minmax(0,1fr))] md:grid-cols-[repeat(4,minmax(0,1fr))_48px_repeat(4,minmax(0,1fr))] gap-x-1 md:gap-x-2 items-center';
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
// The lamb that runs along the bar is finished but held back for now. While this is off, no runner
// renders and the logo's lamb stays at full opacity, so the bar fills exactly as it did before.
const LAMB_RUNS = false;

const RUNNER_MIN = 18;
const RUNNER_MAX = 24;
/** The lamb's width for a height of `h`, in the logomark's proportions. */
const lambWidth = (h: number) => (h * 356) / 422;

// The logomark is pixel art on a 32 by 38 grid, and the runner is a set of sprite frames drawn from
// this map of it. Each letter marks a part that the frames pose on its own: `#` the body, `h` the
// head, `e` the ear, `t` the tail, `a` and `b` the near and far hind legs, and `f` the folded
// forelegs. Every frame but the logomark itself leaves out the logomark's legs and draws its own. A
// capital letter is a cell of the parent part that the child also draws, so a joint stays filled as
// the child turns.
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
 *  A pose either keeps the logomark's own legs or draws all four legs fresh from the hip or shoulder
 *  to a hoof at `hooves`, in the canvas's cells. A hoof at `GROUND` is planted on the bar. `dust` is
 *  a few cells of dust. */
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
  a: [6, 25.5],
  b: [9, 25.2],
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

/** A pose with its legs drawn to hooves given as a step from under each joint, where the pose puts
 *  the joint, and a height. */
function posed(rig: Rig, steps: Record<Leg, Point>): Rig {
  const body = bodyOf(rig);
  const at = (leg: Leg): Point => [
    apply(body, JOINTS[leg])[0] + steps[leg][0],
    steps[leg][1],
  ];
  return { ...rig, hooves: { a: at('a'), b: at('b'), f: at('f'), g: at('g') } };
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
    tail: -4 - 6 * wave(0.65),
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
  // Crouched on the logo, in the logomark's pose, to spring out of it: hind legs bent under it and
  // forelegs folded.
  posed(
    { dy: 0, ear: 8, head: 10, pitch: 3, sx: 1.08, sy: 0.86, tail: 4 },
    { a: [0.5, GROUND], b: [1, GROUND], f: [3, 31], g: [2, 32] },
  ),
  // Springing out of the logo: hind legs pushing a little back, forelegs reaching forward, nose up
  // and stretched.
  posed(
    { dy: 0, ear: -26, head: -8, pitch: -14, sx: 0.92, sy: 1.14, tail: -8 },
    { a: [-1.5, GROUND], b: [-1, 37], f: [6, 27], g: [5, 28] },
  ),
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
    tail: -8,
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
    tail: -10,
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
    tail: -8,
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
    tail: 10,
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
      tail: 10,
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
    tail: 8,
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

/** How far back the rump reaches, in the level body's cells, at height `y`: a smooth curve that is
 *  furthest back halfway between the back and the belly. */
const rumpEdge = (y: number) => 2.5 + 3.5 * ((y - 20) / 7) ** 2;

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
  const joints: [string, Affine][] = [
    ['#ABFHT', body],
    ['hEH', head],
    ['eE', multiply(head, turn(rig.ear, 22, 3.5))],
    ['tT', multiply(body, turn(rig.tail, 3.5, 16))],
  ];
  if (!rig.hooves) joints.push(['abf', body]);
  const parts = joints.map(([keep, m]): [string, Affine] => [keep, invert(m)]);
  const limbs = rig.hooves
    ? (Object.keys(JOINTS) as Leg[]).map((leg) =>
        limb(apply(body, JOINTS[leg]), rig.hooves?.[leg] ?? [0, 0]),
      )
    : [];
  // Turned level, the logomark's leaping haunch bulges out behind the rump. Every frame but the
  // logomark's own trims the body's back to the curve `rumpEdge` gives and fills the notch above the
  // haunch, so the rump reads as one rounded edge. The tail may reach a cell past the curve.
  const level = multiply(turn(LEVEL, 16, 20), invert(body));
  const onLamb = (x: number, y: number) => {
    const hit = (k: number) => {
      const [col, row] = apply(parts[k][1], [x, y]).map(Math.floor);
      return parts[k][0].includes(LAMB[row]?.[col] ?? '.');
    };
    if (rig === LOGO_POSE) return parts.some((_, k) => hit(k));
    const [bx, by] = apply(level, [x, y]);
    const edge = rumpEdge(by);
    if (bx >= edge && bx < 9 && by >= 13 && by < 27) return true;
    return parts.some(
      (_, k) =>
        hit(k) && (k === 0 ? bx >= edge : k === 3 ? bx >= edge - 1 : true),
    );
  };
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

/** Milliseconds since the diagram started, every frame while `frame` is on screen, whether it is
 *  held still, and a `replay` that starts it over. Off screen the clock pauses where it is, so the
 *  diagram costs nothing while it's scrolled away. Under reduced motion it holds at the end. */
function useLap(frame: RefObject<HTMLElement | null>) {
  const [t, setT] = useState(0);
  const [still, setStill] = useState(false);
  const clock = useRef(0);
  // Bumped by `replay`, which starts the clock again from 0.
  const [run, setRun] = useState(0);
  const replay = () => {
    clock.current = 0;
    setT(0);
    setRun((n) => n + 1);
  };
  useEffect(() => {
    void run;
    const el = frame.current;
    if (!el) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setT(DONE_AT + 1);
      setStill(true);
      return;
    }
    let id = 0;
    let last = 0;
    // It plays once: at DONE_AT the clock stops and the diagram stops re-rendering. The glimmer
    // that runs from then on is a CSS animation.
    const tick = (now: number) => {
      if (last) clock.current += Math.min(now - last, 100);
      last = now;
      if (clock.current >= DONE_AT) {
        clock.current = DONE_AT;
        setT(DONE_AT);
        id = 0;
        return;
      }
      setT(clock.current);
      id = requestAnimationFrame(tick);
    };
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting && !id && clock.current < DONE_AT) {
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
  }, [frame, run]);
  return { replay, still, t };
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
  /** Where the stages start: past the Language column, which holds BAML's logo. */
  lead: number;
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
function reach(p: number, { a, b, lead, len }: Shape) {
  const n = BEFORE.length;
  if (p <= n) return lead + (p / n) * (a - lead);
  if (p <= n + 0.2) return a + ((p - n) / 0.2) * (b - a);
  return b + ((p - n - 0.2) / (AFTER.length - 0.2)) * (len - b);
}

/** The inverse of `reach`: how many stages the lap has filled when the fill is `x` pixels along. */
function stagesAt(x: number, { a, b, lead, len }: Shape) {
  const n = BEFORE.length;
  if (x <= a) return (Math.max(0, x - lead) / (a - lead)) * n;
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
  glim,
  t,
  vertical = false,
}: {
  full: boolean;
  home?: RefObject<HTMLElement | null>;
  p: number;
  style?: CSSProperties;
  /** Whether the glimmer circles the loop. */
  glim: boolean;
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
      // The bar starts at the first stage's track; the neck runs from the end of the last stage
      // before the line to the start of Trace, gaps included.
      // The bar starts in the Language column, where BAML's logo sits; the fill starts past it.
      const lead = tracks[0] + gap;
      let a = tracks[0];
      for (let i = 1; i <= BEFORE.length; i++) a += gap + tracks[i];
      const b = a + gap + tracks[BEFORE.length + 1] + gap;
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
        lead,
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

  // Where the Improve line joins the bar, as insets from its ends. On the wide bar the line spans
  // the stages with a 4% margin; on the phone's it starts 18px into the first stage and ends 18px
  // before the bar's end.
  const delay = useGlintDelay(glim);
  const loop: CSSProperties | undefined = shape?.len
    ? vertical
      ? { bottom: 18, top: shape.lead + 18 }
      : {
          left: shape.lead + 0.04 * (shape.len - shape.lead),
          right: 0.04 * (shape.len - shape.lead),
        }
    : undefined;

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
      {/* The glimmer runs only between where the Improve line joins the bar, so the light comes in
          from the line at build, runs the pipe, and leaves into the line at the end: one circuit. */}
      <span
        aria-hidden
        className={`absolute inset-0 pointer-events-none opacity-0 ${glim && delay ? (vertical ? 'tracker-glint-down' : 'tracker-glint-across') : ''}`}
        style={{ ...loop, animationDelay: delay }}
      >
        <span
          className={`absolute left-0 top-0 ${vertical ? 'inset-x-0 h-[10%]' : 'inset-y-0 w-[10%]'}`}
          style={{
            background: `linear-gradient(${vertical ? 180 : 90}deg, transparent, rgb(from var(--ds-accent-ink) r g b / 0.6), transparent)`,
            filter: 'blur(3px)',
          }}
        />
      </span>
    </>
  );

  return (
    <div
      className={vertical ? 'relative self-stretch' : `relative ${ROW}`}
      ref={ref}
      style={style ?? { gridColumn: '1 / -1' }}
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

/** The reveal: it lands once BAML's bar has crossed the line and filled, right above that bar. */
function Until({ on, size = 'text-[15px]' }: { on: boolean; size?: string }) {
  return (
    <span
      className={`${size} font-medium text-(--ds-accent-ink) transition-opacity duration-500`}
      style={{ opacity: on ? 1 : 0 }}
    >
      Until now.
    </span>
  );
}

/** The glimmer's cycle, which home.css's `tracker-glint-*` keyframes also run on. */
const GLINT_MS = 3200;

/** A negative animation delay that puts a glimmer animation starting now at the same point in its
 *  cycle as every other one, wherever the page clock is: the bar's half and the Improve line's half
 *  stay in step even when one mounts later than the other. Fixed once per start, so a re-render
 *  doesn't move it. */
function useGlintDelay(on: boolean) {
  const [delay, setDelay] = useState<string | undefined>(undefined);
  useEffect(() => {
    if (!on) {
      setDelay(undefined);
      return;
    }
    const now = Number(document.timeline.currentTime ?? performance.now());
    setDelay(`${-(now % GLINT_MS)}ms`);
  }, [on]);
  return delay;
}

/** The glimmer on its way back along the Improve line: a dash of light that runs the whole line,
 *  corners and all, from BAML's last stage back to build, timed by CSS to follow the bar's sweep.
 *  The line is measured so its corners keep their radius. `vertical` is the phone's line, which
 *  opens to the left. */
function Return({ on, vertical }: { on: boolean; vertical?: boolean }) {
  const box = useRef<SVGSVGElement>(null);
  const delay = useGlintDelay(on);
  const [size, setSize] = useState<{ h: number; w: number } | null>(null);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => {
      const { height: h, width: w } = el.getBoundingClientRect();
      setSize({ h, w });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  let d = '';
  if (size) {
    const { h, w } = size;
    // Half the 1px border in, so the light sits on the line.
    const e = 0.5;
    if (vertical) {
      const rr = 10;
      d = `M 0 ${h - e} H ${w - e - rr} A ${rr} ${rr} 0 0 0 ${w - e} ${h - e - rr} V ${e + rr} A ${rr} ${rr} 0 0 0 ${w - e - rr} ${e} H 0`;
    } else {
      const rr = 12;
      d = `M ${w - e} 0 V ${h - e - rr} A ${rr} ${rr} 0 0 1 ${w - e - rr} ${h - e} H ${e + rr} A ${rr} ${rr} 0 0 1 ${e} ${h - e - rr} V 0`;
    }
  }
  return (
    // An absolutely placed <svg> keeps its default 300 by 150 size whatever its insets, so a span
    // takes the line's box and the svg fills it.
    <span
      aria-hidden
      className={`absolute pointer-events-none ${vertical ? 'inset-y-[18px] -left-1.5 right-0.5' : 'inset-0'}`}
    >
      <svg className="absolute inset-0 size-full overflow-visible" ref={box}>
        <title>The glimmer on the Improve line</title>
        {on && d && delay && (
          <path
            className="tracker-glint-return"
            d={d}
            fill="none"
            pathLength={100}
            stroke="var(--ds-accent-ink)"
            strokeLinecap="round"
            strokeWidth={2}
            style={{ animationDelay: delay }}
          />
        )}
      </svg>
    </span>
  );
}

/** How big a language's logo is drawn: Go's is a wide wordmark, so it gets more room. */
const logoSize = (lang: { name: string }, size: number) =>
  lang.name === 'Go' ? Math.round(size * 1.5) : size;

/** A language's logo, in the ink color. */
function LangLogo({ mark, size }: { mark: Mark; size: number }) {
  return (
    // biome-ignore lint/a11y/noSvgWithoutTitle: decorative, the language's name is beside it
    <svg
      aria-hidden
      className="shrink-0"
      fill="currentColor"
      height={size}
      viewBox={mark.viewBox ?? '0 0 24 24'}
      width={size}
    >
      <path d={mark.path} />
    </svg>
  );
}

/** A step a language covers with several separate tools: each tool in its own outlined square,
 *  side by side, so the cell reads as a fragmented ecosystem rather than one tool. Below 768px
 *  the logos don't fit, so it is one empty dashed cell like the others. */
function Tools({
  marks,
  merged,
  pick = null,
  quiet,
  style,
}: {
  marks: Mark[];
  /** One dashed box for all the tools, like the products past the line, instead of a square each. */
  merged?: boolean;
  /** The shuffle's look: every tool muted until it's the pick. */
  quiet?: boolean;
  /** The tool this stack is using, brought up to full strength; null leaves them all muted. */
  pick?: number | null;
  style: CSSProperties;
}) {
  return (
    <div
      className={`${ROW} grid grid-flow-col auto-cols-fr gap-1 ${merged ? 'rounded-(--ds-card-radius) border border-dashed border-(--ds-line) px-1' : 'max-md:rounded-(--ds-card-radius) max-md:border max-md:border-dashed max-md:border-(--ds-line)'}`}
      style={style}
    >
      {marks.map((m, i) => (
        <div
          className={`hidden md:flex items-center justify-center text-(--ds-ink)/80 transition-[opacity,transform] duration-300 ease-out ${merged ? '' : 'rounded-(--ds-card-radius) border border-dashed border-(--ds-line)'}`}
          key={m.name}
          style={
            quiet
              ? {
                  opacity: pick === i ? 1 : UNPICKED,
                  transform: pick === i ? PICK_GROW : 'scale(1)',
                }
              : undefined
          }
        >
          <MarkTile mark={m} size={18} />
        </div>
      ))}
    </div>
  );
}

/** How far along its journey each language's line has drawn, 0 to 1, when the lap has filled `p`
 *  stages. The rows start a beat apart, so the lines read as separate journeys. */
function journeyAt(p: number, row: number) {
  const lag = row * 0.25;
  return clamp(
    (p - BEFORE.length - lag) / (AFTER.length - (LANGUAGES.length - 1) * 0.25),
  );
}

/** The transit take on production: a slot per language row in every stage, tools in the slots, and
 *  each language's line running level out of its own row from the deploy line, jogging in the gaps
 *  between stages only where it picks a tool in another row. `lead` is how far before the first
 *  stage the lines start, so they leave from the deploy line. */
function Transit({
  className = '',
  faint,
  gap,
  lead,
  p,
  routes = ONE_EACH,
  style,
  vertical,
}: {
  className?: string;
  /** Thin, quiet lines, for many routes at once. */
  faint?: boolean;
  gap: number;
  lead: number;
  p: number;
  routes?: Route[];
  style: CSSProperties;
  vertical?: boolean;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [paths, setPaths] = useState<string[]>([]);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => {
      const rect = el.getBoundingClientRect();
      if (!rect.width) return;
      // Each slot's entry and exit along the journey, and its middle across it.
      const slot = (lane: number, row: number) => {
        const b = el
          .querySelector(`[data-slot="${lane}-${row}"]`)
          ?.getBoundingClientRect();
        if (!b) return null;
        return vertical
          ? {
              c: b.left + b.width / 2 - rect.left,
              in: b.top - rect.top,
              out: b.bottom - rect.top,
            }
          : {
              c: b.top + b.height / 2 - rect.top,
              in: b.left - rect.left,
              out: b.right - rect.left,
            };
      };
      setPaths(
        routes.map(({ from, picks: line }) => {
          const first = slot(0, from);
          if (!first) return '';
          const pt = (m: number, c: number) =>
            vertical ? `${c} ${m}` : `${m} ${c}`;
          let m = -lead;
          let c = first.c;
          let d = `M ${pt(m, c)}`;
          line.forEach((pick, lane) => {
            const to = slot(lane, pick);
            if (!to) return;
            if (Math.abs(to.c - c) < 0.5) {
              d += ` L ${pt(to.in, to.c)}`;
            } else {
              // A jog in the gap before the stage: level, a smooth S, level.
              const mid = (m + to.in) / 2;
              const half = Math.min((to.in - m) / 2, 18);
              d += ` L ${pt(mid - half, c)} C ${pt(mid, c)} ${pt(mid, to.c)} ${pt(mid + half, to.c)} L ${pt(to.in, to.c)}`;
            }
            d += ` L ${pt(to.out, to.c)}`;
            m = to.out;
            c = to.c;
          });
          return d;
        }),
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [lead, vertical, routes]);

  // Routes from the same row start a beat apart.
  const drawn = routes.map(({ from }, i) => {
    const nth = routes.slice(0, i).filter((r) => r.from === from).length;
    return journeyAt(p - nth * 0.3, from);
  });
  const lit = (lane: number, row: number) =>
    routes.some(
      ({ picks }, i) =>
        picks[lane] === row && drawn[i] >= (lane + 0.5) / AFTER.length,
    );
  return (
    <div className={`relative ${className}`} ref={box} style={style}>
      <svg
        aria-hidden
        className="absolute inset-0 size-full overflow-visible pointer-events-none"
      >
        <title>Each language's route through production</title>
        {paths.map((d, row) => (
          <path
            d={d}
            fill="none"
            // biome-ignore lint/suspicious/noArrayIndexKey: one path per language, in order
            key={row}
            pathLength={1}
            stroke={`rgb(from var(--ds-ink) r g b / ${faint ? 0.28 : 0.55})`}
            strokeDasharray="1 1"
            strokeDashoffset={1 - drawn[row]}
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={faint ? 1.25 : 2}
          />
        ))}
      </svg>
      <div
        className={`relative grid h-full ${vertical ? 'grid-rows-4' : 'grid-cols-4'}`}
        style={{ gap }}
      >
        {SLOTS.map((slots, lane) => (
          <div
            className={`grid ${vertical ? 'grid-cols-4' : 'grid-rows-4'}`}
            key={AFTER[lane]}
            style={{ gap }}
          >
            {slots.map((m, row) => (
              <span
                className={`grid place-items-center ${vertical ? 'py-1' : 'px-[18%]'}`}
                // biome-ignore lint/suspicious/noArrayIndexKey: slots are fixed, one per language row
                key={row}
              >
                <span
                  className="grid size-full place-items-center rounded-(--ds-card-radius) border border-dashed bg-(--ds-ground) transition-[border-color,background-color] duration-500"
                  data-slot={`${lane}-${row}`}
                  style={{
                    background: lit(lane, row)
                      ? 'color-mix(in srgb, var(--ds-ground), var(--ds-ink) 10%)'
                      : undefined,
                    borderColor: lit(lane, row)
                      ? 'rgb(from var(--ds-ink) r g b / 0.5)'
                      : m
                        ? 'var(--ds-line)'
                        : 'rgb(from var(--ds-line) r g b / 0.5)',
                    borderStyle: lit(lane, row) ? 'solid' : 'dashed',
                  }}
                >
                  {m && <MarkTile mark={m} size={vertical ? 13 : 18} />}
                </span>
              </span>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

/** The fill take: a cell for every language in every stage, each a padded box of empty squares,
 *  like TypeScript's tool cells. As the lap reaches a stage, each row's picks pop into its squares,
 *  the rows a beat apart; the squares left over stay empty. */
function Squares({
  className = '',
  gap,
  p,
  style,
}: {
  className?: string;
  gap: number;
  p: number;
  style: CSSProperties;
}) {
  return (
    <div className={`grid grid-cols-4 ${className}`} style={{ ...style, gap }}>
      {AFTER.map((stage, lane) => (
        <div className="grid grid-rows-4" key={stage} style={{ gap }}>
          {LANGUAGES.map((lang, row) => {
            const at = stageAt(p, lane + row * 0.12);
            const picks = PICKS[row][lane];
            return (
              <div
                className="grid grid-cols-3 gap-1 rounded-(--ds-card-radius) border border-dashed border-(--ds-line) p-1"
                key={lang.name}
              >
                {Array.from({ length: SQUARES }, (_, k) => {
                  const m = picks[k];
                  const on = !!m && at >= 0.25 + k * 0.3;
                  return (
                    <span
                      className="grid place-items-center rounded-[5px] border transition-[background-color,border-color] duration-300"
                      // biome-ignore lint/suspicious/noArrayIndexKey: fixed squares in a cell
                      key={k}
                      style={{
                        background: on
                          ? 'rgb(from var(--ds-ink) r g b / 0.1)'
                          : 'transparent',
                        borderColor: on
                          ? 'rgb(from var(--ds-ink) r g b / 0.3)'
                          : 'rgb(from var(--ds-ink) r g b / 0.08)',
                        borderStyle: on ? 'solid' : 'dashed',
                      }}
                    >
                      {m && (
                        <span
                          className="grid place-items-center transition-[opacity,transform] duration-300 ease-out"
                          style={{
                            opacity: on ? 1 : 0,
                            transform: on ? 'scale(1)' : 'scale(0.6)',
                          }}
                        >
                          <MarkTile mark={m} size={16} />
                        </span>
                      )}
                    </span>
                  );
                })}
              </div>
            );
          })}
        </div>
      ))}
    </div>
  );
}

/** The reveal take: the original columns, every stage's boxes there from the start, and the logos
 *  inside popping in one at a time as the lap reaches the stage. */
function Reveal({
  className = '',
  gap,
  p,
  picks = null,
  quiet,
  style,
  vertical,
}: {
  /** The shuffle's look: every product rests muted, and only the stack's picks come up. */
  quiet?: boolean;
  /** The phone's layout: each stage a row, its boxes side by side, without the boxes' labels. */
  vertical?: boolean;
  className?: string;
  /** The stack being shown: one tool per stage at full strength, the rest of the pool faded; null shows
   *  the whole pool alike. */
  picks?: Mark[] | null;
  gap: number;
  p: number;
  style: CSSProperties;
}) {
  return (
    <div
      className={`grid ${vertical ? 'grid-rows-4' : 'grid-cols-4'} ${className}`}
      style={{ ...style, gap }}
    >
      {ZONED.map((zones, lane) => {
        const at = stageAt(p, lane);
        const all = zones.flat();
        return (
          <div
            className={`flex ${vertical ? 'flex-row' : 'flex-col'}`}
            key={AFTER[lane]}
            style={{ gap }}
          >
            {zones.map((marks, z) => (
              <div
                className={`relative flex flex-1 flex-wrap content-center items-center justify-center rounded-(--ds-card-radius) border border-dashed border-(--ds-line) text-(--ds-ink)/75 ${vertical ? 'gap-1.5 px-1' : 'gap-3 px-2 pt-3'}`}
                key={marks[0].name}
              >
                {!vertical && ZONE_NAMES[lane][z] && (
                  <span
                    className="absolute left-2 top-1.5 text-[10.5px] leading-none"
                    style={{ color: MUTED }}
                  >
                    {ZONE_NAMES[lane][z]}
                  </span>
                )}
                {marks.map((m) => {
                  const k = all.indexOf(m);
                  const on = at >= 0.15 + (k / all.length) * 0.75;
                  const picked = picks !== null && picks[lane] === m;
                  const lit = quiet ? picked : picks === null || picked;
                  return (
                    <span
                      className="grid place-items-center transition-[opacity,transform] duration-300 ease-out"
                      key={m.name}
                      style={{
                        opacity: on ? (lit ? 1 : UNPICKED) : 0,
                        transform: on
                          ? quiet && picked
                            ? PICK_GROW
                            : 'scale(1)'
                          : 'scale(0.6)',
                      }}
                    >
                      <MarkTile
                        mark={m}
                        size={vertical ? 13 : zones.length > 1 ? 20 : 22}
                      />
                    </span>
                  );
                })}
              </div>
            ))}
          </div>
        );
      })}
    </div>
  );
}

/** Production: every tool once, in its stage's lane, and a line per language through the tools it
 *  picks, drawn as `p` crosses the stages. Lanes run across in columns on the wide diagram and down
 *  in rows on the phone (`vertical`). Each line starts level with its language's row (or column)
 *  and curves from tool to tool. Tools a line has reached light up; the rest stay dim. */
function Journeys({
  className = '',
  gap,
  p,
  style,
  vertical,
}: {
  className?: string;
  /** The gap between lanes and between language rows, in pixels. */
  gap: number;
  p: number;
  style: CSSProperties;
  vertical?: boolean;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [paths, setPaths] = useState<string[]>([]);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const measure = () => {
      const rect = el.getBoundingClientRect();
      if (!rect.width) return;
      const cross = vertical ? rect.width : rect.height;
      const n = LANGUAGES.length;
      const band = (cross - gap * (n - 1)) / n;
      const at = (tile: Element) => {
        const b = tile.getBoundingClientRect();
        const x = b.left + b.width / 2 - rect.left;
        const y = b.top + b.height / 2 - rect.top;
        return vertical ? [y, x] : [x, y];
      };
      const tiles = (lane: number, i: number) =>
        el.querySelector(`[data-tile="${lane}-${i}"]`);
      setPaths(
        ROUTES.map((route, row) => {
          // Main axis along the journey, cross axis across the languages.
          const pts: number[][] = [[-gap, row * (band + gap) + band / 2]];
          route.forEach((pick, lane) => {
            const t = tiles(lane, pick);
            if (t) pts.push(at(t));
          });
          let d = '';
          pts.forEach(([m, c], i) => {
            const [x, y] = vertical ? [c, m] : [m, c];
            if (i === 0) {
              d = `M ${x} ${y}`;
              return;
            }
            const [pm, pc] = pts[i - 1];
            const mid = (pm + m) / 2;
            d += vertical
              ? ` C ${pc} ${mid} ${c} ${mid} ${c} ${m}`
              : ` C ${mid} ${pc} ${mid} ${c} ${m} ${c}`;
          });
          return d;
        }),
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [gap, vertical]);

  const drawn = ROUTES.map((_, row) => journeyAt(p, row));
  // A tool lights once a line has reached it: roughly when the line is that far through its stages.
  const lit = (lane: number, i: number) =>
    ROUTES.some(
      (route, row) =>
        route[lane] === i && drawn[row] >= (lane + 0.5) / AFTER.length,
    );
  return (
    <div className={`relative ${className}`} ref={box} style={style}>
      <svg
        aria-hidden
        className="absolute inset-0 size-full overflow-visible pointer-events-none"
      >
        <title>Each language's journey through production</title>
        {paths.map((d, row) => (
          <path
            d={d}
            fill="none"
            // biome-ignore lint/suspicious/noArrayIndexKey: one path per language, in order
            key={row}
            pathLength={1}
            stroke="rgb(from var(--ds-ink) r g b / 0.5)"
            strokeDasharray="1 1"
            strokeDashoffset={1 - drawn[row]}
            strokeLinecap="round"
            strokeWidth={1.5}
          />
        ))}
      </svg>
      <div
        className={`relative grid h-full ${vertical ? 'grid-rows-4' : 'grid-cols-4'}`}
        style={{ gap }}
      >
        {TOOLS.map((tools, lane) => (
          <div
            className={`flex ${vertical ? 'flex-row' : 'flex-col'} items-center justify-around`}
            key={AFTER[lane]}
          >
            {tools.map((m, i) => (
              <span
                className={`grid place-items-center rounded-(--ds-card-radius) border border-dashed bg-(--ds-ground) transition-[opacity,border-color] duration-500 ${vertical ? 'size-7' : 'size-10'}`}
                data-tile={`${lane}-${i}`}
                key={m.name}
                style={{
                  borderColor: lit(lane, i)
                    ? 'rgb(from var(--ds-ink) r g b / 0.45)'
                    : 'var(--ds-line)',
                  opacity: lit(lane, i) ? 1 : 0.45,
                }}
              >
                <MarkTile mark={m} size={vertical ? 13 : 18} />
              </span>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

/** The lap's state at one moment, shared by the wide diagram and the phone's. */
type Lap = {
  back: boolean;
  /** Which take on the production side to draw. */
  production: Production;
  /** Whether the glimmer circles BAML's loop. */
  glim: boolean;
  /** Whether BAML's row has popped in. */
  baml: boolean;
  /** How many stages BAML's bar has filled, in stage widths. */
  q: number;
  until: boolean;
  deployed: boolean;
  full: boolean;
  handed: boolean;
  lineColor: string;
  p: number;
  style: string;
};

// The phone's diagram turns the wide one on its side: stages run down the rows, the languages and
// BAML across the columns. The columns are the stage names, the three languages, BAML, and a narrow
// one for the line back from the last stage to build. The rows are the column heads, Build and Test,
// the deploy line, then the four stages past it; `column` numbers them the same way.
const VGRID =
  'grid grid-cols-[60px_repeat(4,minmax(0,1fr))_56px_22px] grid-rows-[32px_repeat(3,32px)_20px_repeat(4,36px)] gap-1.5';

/** A cell for a step with no single tool: small gray boxes packed together, lighting up as the lap
 *  reaches them. Logos don't fit in a phone's cell, so the pile says it instead. */
function Pile({
  fill,
  row,
  lane,
}: {
  fill: number;
  lane: number;
  row: number;
}) {
  return (
    <span
      className="grid grid-cols-2 grid-rows-2 gap-0.5 rounded-(--ds-card-radius) border border-dashed border-(--ds-line) p-0.5"
      style={{ gridColumn: lane, gridRow: row }}
    >
      {[0, 1, 2, 3].map((i) => (
        <span
          className="rounded-[2px] bg-(--ds-ink)/25 transition-opacity duration-300"
          key={i}
          style={{ opacity: fill > i / 4 ? 1 : 0.3 }}
        />
      ))}
    </span>
  );
}

/** The hero diagram on a phone, with BAML's bar filling from the top down. */
function Vertical({
  back,
  production,
  glim,
  baml,
  deployed,
  full,
  lineColor,
  p,
  q,
  style,
  until,
}: Lap) {
  const deployRow = column(BEFORE.length) - 1;
  return (
    // Its cells are about as wide as they are tall, so it takes a smaller corner radius than the
    // style's, which would round them into pills.
    <div className="sm:hidden [--ds-card-radius:5px]">
      <div className={`${VGRID} relative`}>
        <span
          className="self-center text-[12px] leading-tight"
          style={{ color: MUTED, gridColumn: 1, gridRow: 1 }}
        >
          Language
        </span>
        {/* Each language and the steps its own toolchain covers, as one block down its column. */}
        {LANGUAGES.map((lang, i) => (
          <Card
            className="relative overflow-hidden flex flex-col items-center text-(--ds-ink)/85"
            key={lang.name}
            style={{
              gridColumn: i + 2,
              gridRow: `1 / span ${lang.unified + 1}`,
            }}
          >
            {lang.unified > 0 && (
              <Fill fill={clamp(p / lang.unified)} vertical />
            )}
            <span className="relative flex h-8 shrink-0 items-center">
              <LangLogo mark={lang.logo} size={logoSize(lang, 14)} />
            </span>
            <span className="sr-only">{lang.name}</span>
          </Card>
        ))}
        {/* BAML's logo sits at the top of its bar, which runs from the language row down, at about
            the size of the languages' logos beside it. */}
        <span
          className="relative z-10 flex self-center justify-self-center"
          style={{ gridColumn: BAML_COL, gridRow: 1, ...popIn(baml) }}
        >
          <Image
            alt="BAML"
            className="h-[17px] w-auto"
            height={17}
            src={bamlMark}
            width={14}
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
              opacity: 1,
            }}
          >
            {SHORT[s] ?? s}
          </span>
        ))}

        {/* The deploy line, across the languages only: BAML's row comes in after it. */}
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
            gridColumn: `2 / ${BAML_COL}`,
            gridRow: deployRow,
          }}
        />

        {LANGUAGES.map((lang, i) => (
          <Fragment key={lang.name}>
            {lang.steps.map((step, k) =>
              k < lang.unified ? null : 'command' in step ? (
                <Card
                  className="relative overflow-hidden"
                  key={BEFORE[k]}
                  style={{ gridColumn: i + 2, gridRow: column(k) }}
                >
                  <Fill fill={clamp(p - k)} vertical />
                </Card>
              ) : 'marks' in step ? (
                <Pile
                  fill={clamp(p - k)}
                  key={BEFORE[k]}
                  lane={i + 2}
                  row={column(k)}
                />
              ) : (
                // Nothing of its own: a dashed cell, like the products past the line.
                <span
                  className="rounded-(--ds-card-radius) border border-dashed border-(--ds-line)"
                  key={BEFORE[k]}
                  style={{ gridColumn: i + 2, gridRow: column(k) }}
                />
              ),
            )}
          </Fragment>
        ))}

        {production === 'transit' || production === 'tangle' ? (
          <Transit
            faint={production === 'tangle'}
            gap={6}
            lead={13}
            p={p}
            routes={production === 'tangle' ? TANGLE : ONE_EACH}
            style={{
              gridColumn: `2 / ${BAML_COL}`,
              gridRow: `${column(BEFORE.length)} / span ${AFTER.length}`,
            }}
            vertical
          />
        ) : production === 'reveal' || production === 'shuffle' ? (
          <Reveal
            gap={6}
            p={p}
            style={{
              gridColumn: `2 / ${BAML_COL}`,
              gridRow: `${column(BEFORE.length)} / span ${AFTER.length}`,
            }}
            vertical
          />
        ) : (
          <Journeys
            gap={6}
            p={p}
            style={{
              gridColumn: `2 / ${BAML_COL}`,
              gridRow: `${column(BEFORE.length)} / span ${AFTER.length}`,
            }}
            vertical
          />
        )}

        <BamlBar
          full={full}
          glim={glim}
          key={style}
          p={q}
          style={{
            gridColumn: BAML_COL,
            gridRow: '1 / -1',
            ...popIn(baml),
          }}
          vertical
        />
        {/* No commands on a phone: there isn't room, and the point is the one lit bar running the
            whole way down beside the languages' broken-up columns. */}

        {/* BAML's line runs back up from the end of its column to build. */}
        <span
          aria-hidden
          className="relative"
          style={{ gridColumn: BAML_COL + 1, gridRow: `${column(0)} / -1` }}
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
          <Return on={glim} vertical />
        </span>
        {/* Inside the loop, level with the deploy line and just inside the loop's line. */}
        <span
          className="relative self-center justify-self-end mr-2 rotate-180 text-[11px] leading-none text-(--ds-accent-ink) [writing-mode:vertical-rl] whitespace-nowrap transition-opacity duration-500"
          style={{
            gridColumn: BAML_COL + 1,
            gridRow: deployRow,
            opacity: back ? 1 : 0,
          }}
        >
          Improve
        </span>
      </div>
      <p className="mt-3">
        <Until on={until} />
      </p>
    </div>
  );
}

/** The v4 hero diagram in the given /diagram-styles style, played once from build to production,
 *  with a Replay button at the end and a glimmer circling BAML's loop from then on. */
export default function Tracker({
  production = 'reveal',
  style,
}: {
  production?: Production;
  style: string;
}) {
  const root = useRef<HTMLDivElement>(null);
  const lamb = useRef<HTMLImageElement>(null);
  const { replay, still, t } = useLap(root);
  // The deploy line runs from under its label to the bottom of the language rows, measured.
  const rows = useRef<HTMLDivElement>(null);
  const line = useRef<HTMLDivElement>(null);
  const [deployH, setDeployH] = useState<number | null>(null);
  // Only the layout on screen renders once the width is known, so each frame updates one, not two.
  const wide = useWide();
  useEffect(() => {
    const from = line.current;
    const to = rows.current;
    if (!wide || !from || !to) return;
    const measure = () =>
      setDeployH(
        to.getBoundingClientRect().bottom - from.getBoundingClientRect().top,
      );
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(to);
    return () => observer.disconnect();
  }, [wide]);
  // How many stages the lap has filled, in stage widths.
  // The development side starts full; the production side pops in all at once.
  const p = t >= PROD_AT ? STAGES.length : BEFORE.length;
  const deployed = true;
  const handed = t >= PROD_AT;
  const until = t >= UNTIL_AT;
  const baml = t >= BAML_AT;
  const q = ease(clamp((t - BAML_AT) / BAML_MS)) * STAGES.length;
  const back = t >= FULL_AT + 300;
  const full = t >= FULL_AT;
  const done = t >= DONE_AT;
  // Once it has played, a glimmer circles BAML's loop. Under reduced motion it never starts.
  const glim = done && !still;
  // The shuffle take's project: once every product is revealed, one language at a time.
  const revealed = p >= STAGES.length;
  // Each beat is one stack: a language, and one tool picked in every stage it doesn't cover itself.
  const [beat, setBeat] = useState<number | null>(null);
  useEffect(() => {
    if (production !== 'shuffle' || still || !revealed) {
      setBeat(null);
      return;
    }
    let n = 0;
    setBeat(0);
    const id = setInterval(() => {
      n += 1;
      setBeat(n);
    }, SHUFFLE_BEAT_MS);
    return () => clearInterval(id);
  }, [production, still, revealed]);
  const focus = beat === null ? null : beat % LANGUAGES.length;
  const stack =
    beat === null
      ? null
      : ZONED.map((zones, lane) => {
          const pool = zones.flat();
          return pool[pickOf(beat, lane, pool.length)];
        });
  const lineColor = deployed
    ? 'var(--ds-accent)'
    : 'rgb(from var(--ds-ink) r g b / 0.45)';

  return (
    <div
      className={`tracker ds-theme ds-theme-${style} rounded-[28px] p-2 sm:p-3`}
      ref={root}
    >
      <Frame label="Where each toolchain reaches. C builds with cc and leaves testing to you. TypeScript builds with Vite, webpack, or esbuild, and tests with Jest, Vitest, or Mocha. Go builds and tests with go build and go test, and leaves releasing to you. Rust builds, tests, and releases with cargo. Past deploy, every team mixes and matches its own separate products for tracing, dashboards, alerts, and more, no two alike. BAML covers build, test, release, query, dashboards, alerts, and more itself, and a line runs from production back to build, labelled Improve.">
        {/* Cards take a smaller corner radius than the style's, so they read as boxes, not pills. */}
        <div className="relative p-(--tracker-pad) [--ds-card-radius:8px]">
          {!still && (
            <button
              aria-hidden={!done}
              className="focus-ring absolute right-2 bottom-2 sm:right-auto sm:left-(--tracker-pad) sm:bottom-3 z-10 flex cursor-pointer items-center gap-1.5 rounded-full border border-(--ds-line) px-3 py-1 text-[12.5px] text-(--ds-ink)/75 transition-[opacity,color,border-color] duration-500 hover:border-(--ds-accent) hover:text-(--ds-accent-ink)"
              onClick={replay}
              style={{
                opacity: done ? 1 : 0,
                pointerEvents: done ? 'auto' : 'none',
              }}
              tabIndex={done ? 0 : -1}
              type="button"
            >
              <RotateCcw aria-hidden className="size-3.5" />
              Replay
            </button>
          )}
          {/* The claim the diagram makes, over it from the start; "Until now." answers it below. */}
          <p className="mb-3 md:mb-5 text-center text-[14px] md:text-[18px] lg:text-[20px] font-medium leading-snug text-(--ds-ink)">
            No single stack has ever gone past deploy.
          </p>
          {wide !== true && (
            <Vertical
              back={back}
              baml={baml}
              deployed={deployed}
              full={full}
              glim={glim}
              handed={handed}
              lineColor={lineColor}
              p={p}
              production={production}
              q={q}
              style={style}
              until={until}
            />
          )}
          {wide !== false && (
            <div className="relative hidden sm:block">
              {/* The deploy line, drawn once from under its label to the bottom of the language rows, in
                the gap column, so it runs through them without touching the cells on either side. It
                stops there: the callout sits under it, and BAML's row comes after. */}
              <div
                aria-hidden
                className={`${GRID} absolute inset-x-0 top-(--tracker-head) pointer-events-none`}
                ref={line}
                style={{ height: deployH ?? undefined }}
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

              <div className={`${GRID} h-(--tracker-head) mb-1.5 md:mb-2.5`}>
                <span
                  className="justify-self-center font-mono text-[11px] md:text-[13px] whitespace-nowrap transition-colors duration-500"
                  style={{
                    color: deployed ? 'var(--ds-accent-ink)' : MUTED,
                    gridColumn: column(BEFORE.length) - 1,
                    gridRow: 1,
                  }}
                >
                  deploy
                </span>
                {/* A labelled rule across exactly the columns it heads. */}
                <span
                  className="hidden md:flex items-center gap-3 text-[15px] font-medium text-(--ds-ink)"
                  style={{
                    gridColumn: `1 / span ${BEFORE.length + 1}`,
                    gridRow: 1,
                  }}
                >
                  <span className="h-px flex-1 bg-(--ds-line)" />
                  Development
                  <span className="h-px flex-1 bg-(--ds-line)" />
                </span>
                {/* A labelled rule across exactly the columns it heads. */}
                <span
                  className="hidden md:flex items-center gap-3 text-[15px] font-medium text-(--ds-ink)"
                  style={{
                    gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                    gridRow: 1,
                  }}
                >
                  <span className="h-px flex-1 bg-(--ds-line)" />
                  Production
                  <span className="h-px flex-1 bg-(--ds-line)" />
                </span>
              </div>

              {/* Column labels. On a phone, the two sides of the line are labelled instead. */}
              <div
                className={`${GRID} mb-2 md:mb-3 text-[12.5px] md:text-[14px]`}
                style={{ color: MUTED }}
              >
                <span
                  className="hidden md:block text-center truncate"
                  style={{ gridColumn: 1 }}
                >
                  Language
                </span>
                {STAGES.map((s, i) => (
                  <span
                    className="hidden md:block text-center truncate transition-opacity duration-500"
                    key={s}
                    style={{
                      gridColumn: column(i),
                      opacity: 1,
                    }}
                  >
                    {s}
                  </span>
                ))}
                <span
                  className="md:hidden"
                  style={{ gridColumn: `1 / span ${BEFORE.length + 1}` }}
                >
                  Development
                </span>
                <span
                  className="md:hidden"
                  style={{
                    gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                  }}
                >
                  In production
                </span>
              </div>

              <div className="relative flex flex-col gap-1.5 md:gap-2">
                {/* One grid for the language rows, so each product column can span all three. */}
                <div className={`${GRID} gap-y-1.5 md:gap-y-2`} ref={rows}>
                  {LANGUAGES.map((lang, i) => (
                    <Fragment key={lang.name}>
                      {/* The language and the steps its own toolchain covers, as one block. */}
                      <Card
                        className={`relative ${ROW} overflow-hidden flex items-center min-w-0 text-(--ds-ink)/85 transition-[box-shadow,opacity] duration-300`}
                        style={{
                          gridColumn: `1 / span ${lang.unified + 1}`,
                          gridRow: i + 1,
                          opacity:
                            focus === null || focus === i ? 1 : UNFOCUSED,
                        }}
                      >
                        {lang.unified > 0 && (
                          <Fill fill={clamp(p / lang.unified)} />
                        )}
                        <span
                          className="relative flex shrink-0 justify-center"
                          style={{
                            width: `${100 / (lang.unified + 1)}%`,
                          }}
                        >
                          <LangLogo
                            mark={lang.logo}
                            size={logoSize(lang, 20)}
                          />
                          <span className="sr-only">{lang.name}</span>
                        </span>
                        {lang.tool && (
                          <span className="relative flex-1 text-center font-mono text-[12px] md:text-[13px] text-(--ds-ink)/80">
                            {lang.tool}
                          </span>
                        )}
                      </Card>
                      {lang.steps.map((step, k) =>
                        k < lang.unified ? null : 'command' in step ? (
                          <Card
                            className={`relative ${ROW} overflow-hidden transition-opacity duration-300`}
                            key={BEFORE[k]}
                            style={{
                              gridColumn: column(k),
                              gridRow: i + 1,
                              opacity:
                                focus === null || focus === i ? 1 : UNFOCUSED,
                            }}
                          >
                            <Fill fill={clamp(p - k)} />
                            <Command>{step.command}</Command>
                          </Card>
                        ) : 'marks' in step ? (
                          <Tools
                            key={BEFORE[k]}
                            marks={step.marks}
                            merged={
                              production === 'shuffle' ||
                              production === 'reveal'
                            }
                            pick={
                              beat !== null && focus === i
                                ? pickOf(beat, 10 + k, step.marks.length)
                                : null
                            }
                            quiet={production === 'shuffle'}
                            style={{
                              gridColumn: column(k),
                              gridRow: i + 1,
                              opacity:
                                focus === null || focus === i ? 1 : UNFOCUSED,
                              transition: 'opacity 300ms',
                            }}
                          />
                        ) : (
                          // Nothing of its own: an empty dashed cell, like the products past the line.
                          <div
                            className={`${ROW} rounded-(--ds-card-radius) border border-dashed border-(--ds-line) transition-opacity duration-300`}
                            key={BEFORE[k]}
                            style={{
                              gridColumn: column(k),
                              gridRow: i + 1,
                              opacity:
                                focus === null || focus === i ? 1 : UNFOCUSED,
                            }}
                          />
                        ),
                      )}
                    </Fragment>
                  ))}

                  {/* Past the line, each language row mixes and matches its own products. */}
                  {/* Past the line, every tool once, and each language's own journey through them. */}
                  {production === 'transit' || production === 'tangle' ? (
                    <Transit
                      className="hidden md:block self-stretch"
                      faint={production === 'tangle'}
                      gap={8}
                      lead={32}
                      p={p}
                      routes={production === 'tangle' ? TANGLE : ONE_EACH}
                      style={{
                        gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                        gridRow: `1 / span ${LANGUAGES.length}`,
                      }}
                    />
                  ) : production === 'fill' ? (
                    <Squares
                      className="hidden md:grid self-stretch"
                      gap={8}
                      p={p}
                      style={{
                        gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                        gridRow: `1 / span ${LANGUAGES.length}`,
                      }}
                    />
                  ) : production === 'reveal' || production === 'shuffle' ? (
                    <Reveal
                      className="hidden md:grid self-stretch"
                      gap={8}
                      p={p}
                      picks={stack}
                      quiet={production === 'shuffle'}
                      style={{
                        gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                        gridRow: `1 / span ${LANGUAGES.length}`,
                      }}
                    />
                  ) : (
                    <Journeys
                      className="hidden md:block self-stretch"
                      gap={8}
                      p={p}
                      style={{
                        gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                        gridRow: `1 / span ${LANGUAGES.length}`,
                      }}
                    />
                  )}
                  <p
                    className="self-stretch md:hidden flex items-center justify-center rounded-(--ds-card-radius) border border-dashed border-(--ds-line) px-1 text-center text-[11.5px] transition-opacity duration-500"
                    style={{
                      color: MUTED,
                      gridColumn: `${column(BEFORE.length)} / span ${AFTER.length}`,
                      gridRow: `1 / span ${LANGUAGES.length}`,
                      opacity: handed ? 1 : 0,
                    }}
                  >
                    {PRODUCT_COUNT} separate products
                  </p>
                </div>

                {/* The point of the whole diagram, centered across it, which BAML's bar under it
                answers. */}
                <p className="hidden md:block pt-3 text-center leading-snug">
                  <Until on={until} size="text-[18px] lg:text-[20px]" />
                </p>

                <div
                  className={`${GRID} relative mt-(--tracker-gap)`}
                  style={popIn(baml)}
                >
                  <BamlBar
                    full={full}
                    glim={glim}
                    home={lamb}
                    key={style}
                    p={q}
                    t={still || !LAMB_RUNS ? undefined : t}
                  />
                  {/* The commands sit over the block, in the same columns as the stages above them. */}
                  <div
                    className={`${GRID} absolute inset-0 pointer-events-none`}
                  >
                    {/* BAML's logo sits in the bar itself, in the Language column: BAML is the
                    language and the rest of the stack, one block. The logo is at the brand's 32px
                    minimum; the logomark fits the phone's narrow column, the primary logo the wide
                    one. The lamb is drawn on its own, over the primary logo with the logo's lamb
                    clipped away, at 68.01% across and 31.7px tall, where the primary logo draws it. */}
                    <span
                      className="flex items-center justify-center"
                      style={{ gridColumn: 1, gridRow: 1 }}
                    >
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
                              '--lamb-home':
                                still || !LAMB_RUNS ? 1 : homeOpacity(t),
                            } as CSSProperties
                          }
                          width={27}
                        />
                      </span>
                    </span>
                    {BAML.map((command, i) => (
                      <span
                        className={`${ROW} min-w-0 transition-opacity duration-300`}
                        key={command}
                        style={{
                          gridColumn: column(i),
                          gridRow: 1,
                          opacity: clamp(q - i) > 0.4 ? 1 : 0.35,
                        }}
                      >
                        <Command lit>{command}</Command>
                      </span>
                    ))}
                  </div>
                </div>

                {/* BAML's row runs back from its last stage to build. */}
                <div aria-hidden className={`${GRID} h-(--tracker-ret)`}>
                  {/* Pulled up over the gap above, so the line's ends meet the bar's underside. */}
                  <div
                    className="relative self-start -mt-1.5 md:-mt-2 h-[calc(70%+6px)] md:h-[calc(70%+8px)] mx-[4%]"
                    style={{ gridColumn: '2 / -1', gridRow: 1 }}
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
                    <Return on={glim} />
                  </div>
                  {/* Inside the loop, right under the deploy line and sitting just on the loop's line,
                      so it names the line: production feeding back into development. */}
                  <span
                    className="self-start -mt-1.5 md:-mt-2 h-[calc(70%+6px)] md:h-[calc(70%+8px)] pb-1.5 flex items-end justify-self-center whitespace-nowrap text-[12.5px] md:text-[13px] leading-none text-(--ds-accent-ink) transition-opacity duration-500"
                    style={{
                      gridColumn: column(BEFORE.length) - 1,
                      gridRow: 1,
                      opacity: back ? 1 : 0,
                    }}
                  >
                    Improve
                  </span>
                </div>
              </div>
            </div>
          )}
        </div>
      </Frame>
    </div>
  );
}
