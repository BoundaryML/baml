'use client';

import { useEffect, useMemo, useRef, useState } from 'react';
import { section } from '@/components/home/analytics';
import { Hyper, Pinned } from '@/components/home/pinned';
import { ramp } from '@/components/home/scroll';
import { RED } from '@/components/home/story';

// Nobody could read this much data, and an agent reads all of it. Every number here is illustrative.

/** Noise in [0, 1) for point i, so the figures draw the same way on every render. */
function noise(i: number, seed = 1) {
  const x = Math.sin(i * 12.9898 + seed * 78.233) * 43758.5453;
  return x - Math.floor(x);
}

const BARS_PER_RUN = 13;
const SPANS_PER_RUN = 200;
const MAX_RUNS = 10_000_000;
const WIDE_COLS = 64;
// Boxes this far apart stay visible on a phone, and there are fewer of them to draw.
const PHONE_COLS = 36;
const BAND = 0.07;
const POP = 0.14;
const compact = new Intl.NumberFormat('en-US', {
  maximumSignificantDigits: 3,
  notation: 'compact',
});

const hex = (h: string) =>
  [1, 3, 5].map((i) => Number.parseInt(h.slice(i, i + 2), 16) / 255);

// `upTo` is where each kind ends on a 0 to 1 scale, so some problems are far more common than others.
const KINDS = [
  { color: RED, name: 'Timeouts', upTo: 0.58 },
  { color: '#F5C542', name: 'Bad tool calls', upTo: 0.84 },
  { color: '#4CC9B0', name: 'Retry loops', upTo: 0.95 },
  { color: '#5AA9F5', name: 'GC spikes', upTo: 1 },
];
const COLORS = KINDS.map((k) => k.color);

// Finds, as id, which kind, and how far along the wave reaches them.
type Find = { at: number; id: number; kind: number };

function findsOf(cols: number): Find[] {
  const out: Find[] = [];
  for (let id = 0; id < cols * cols; id++) {
    if (noise(id, 7) > 0.988) {
      const at = ((id % cols) + Math.floor(id / cols)) / (2 * (cols - 1));
      const roll = noise(id, 11);
      out.push({ at, id, kind: KINDS.findIndex((k) => roll < k.upTo) });
    }
  }
  return out;
}

/** Where each box ends up once sorted: the clean ones first, then each kind in a run of its own. */
function sortedPlaces(cols: number, finds: Find[]) {
  const kindOf = new Map(finds.map((f) => [f.id, f.kind]));
  const order = Array.from({ length: cols * cols }, (_, id) => id).sort(
    (a, b) => (kindOf.get(a) ?? -1) - (kindOf.get(b) ?? -1) || a - b,
  );
  const places = new Map<number, [number, number]>();
  order.forEach((id, rank) => {
    places.set(id, [
      (rank % cols) - (id % cols),
      Math.floor(rank / cols) - Math.floor(id / cols),
    ]);
  });
  return places;
}

const VERTEX = `#version 300 es
in vec2 corner;
in vec4 rect;
in vec3 meta;
in vec2 shift;
uniform float scale;
uniform float wave;
uniform float detail;
uniform float sorted;
uniform float cols;
uniform float aspect;
uniform float flashes;
uniform vec3 palette[5];
out vec4 tint;
out vec2 uv;
out float mode;
const float band = ${BAND.toFixed(3)};
const float pop = ${POP.toFixed(3)};
void main() {
  float id = meta.x;
  float kind = meta.y;
  mode = meta.z;
  uv = corner;
  float reach = (mod(id, cols) + floor(id / cols)) / (2.0 * (cols - 1.0));
  float past = wave - reach;
  bool seen = past > 0.0;
  bool bad = kind > 0.5;
  float flash = seen ? (1.0 - smoothstep(0.0, band, past)) * flashes : 0.0;
  float burst = seen ? (1.0 - smoothstep(0.0, pop, past)) * flashes : 0.0;
  vec2 size = rect.zw;
  vec3 color = vec3(0.792, 0.667, 0.949);
  float alpha = seen ? 0.2 : 0.5;
  if (mode < 1.5) {
    if (seen && bad) { color = palette[int(kind)]; alpha = 1.0; size *= 1.0 + burst * 0.9; }
    else { color = mix(color, vec3(1.0), flash * 0.8); alpha = max(alpha, flash * 0.9); }
    alpha *= mode > 0.5 ? detail : 1.0 - detail;
  } else if (mode < 2.5) {
    color = palette[int(kind)];
    size = vec2(9.0, 9.0 * aspect);
    alpha = (seen ? 0.4 + burst * 0.5 : 0.0) * (1.0 - sorted * 0.85);
  } else if (mode > 3.5) {
    // The grid left behind, so the wall stays full while its boxes are in the air.
    alpha = 0.2 * sin(sorted * 3.14159);
  } else {
    color = palette[int(kind)];
    float grown = seen ? clamp(past / pop, 0.0, 1.0) : 0.0;
    size = vec2(1.0 + grown * 7.0, (1.0 + grown * 7.0) * aspect);
    alpha = seen && past < pop ? (1.0 - grown) * flashes : 0.0;
  }
  vec2 middle = rect.xy + rect.zw * 0.5 + shift * sorted;
  vec2 at = (middle + (corner - 0.5) * size) * scale;
  gl_Position = vec4(at.x * 2.0 - 1.0, 1.0 - at.y * 2.0, 0.0, 1.0);
  tint = vec4(color, alpha);
}`;

const FRAGMENT = `#version 300 es
precision mediump float;
in vec4 tint;
in vec2 uv;
in float mode;
out vec4 color;
void main() {
  float a = tint.a;
  float r = length(uv * 2.0 - 1.0);
  if (mode > 3.5) a = tint.a;
  else if (mode > 2.5) a *= smoothstep(0.07, 0.0, abs(r - 0.92));
  else if (mode > 1.5) a *= pow(max(0.0, 1.0 - r), 1.6);
  color = vec4(tint.rgb * a, a);
}`;

/** Every box and bar in the wall, then a glow and a ring for each find. */
function wallInstances(
  cols: number,
  finds: Find[],
  places: Map<number, [number, number]>,
) {
  const out: number[] = [];
  const pad = 0.08;
  const inner = 1 - pad * 2;
  const kindOf = new Map(finds.map((f) => [f.id, f.kind + 1]));
  const put = (r: number[], id: number, mode: number) => {
    const [dx, dy] = places.get(id) ?? [0, 0];
    out.push(...r, id, kindOf.get(id) ?? 0, mode, dx, dy);
  };
  for (let id = 0; id < cols * cols; id++) {
    out.push(
      (id % cols) + pad,
      Math.floor(id / cols) + pad,
      inner,
      inner,
      id,
      0,
      4,
      0,
      0,
    );
  }
  for (let id = 0; id < cols * cols; id++) {
    const x = id % cols;
    const y = Math.floor(id / cols);
    put([x + pad, y + pad, inner, inner], id, 0);
    for (let b = 0; b < BARS_PER_RUN; b++) {
      put(
        [
          x + pad + (noise(id * 13 + b, 2) * 0.6 + b * 0.02) * inner,
          y + pad + (b * inner) / BARS_PER_RUN,
          (0.12 + noise(id * 13 + b, 3) * 0.3) * inner,
          (inner / BARS_PER_RUN) * 0.8,
        ],
        id,
        1,
      );
    }
  }
  for (const f of finds) {
    const r = [f.id % cols, Math.floor(f.id / cols), 1, 1];
    put(r, f.id, 2);
    put(r, f.id, 3);
  }
  return new Float32Array(out);
}

type Wall = {
  draw: (scale: number, wave: number, sorted: number) => void;
  stop: () => void;
};

/** The geometry goes to the GPU once. After that, a frame only sets a few numbers. */
function startWall(
  canvas: HTMLCanvasElement,
  cols: number,
  data: Float32Array,
  colors: string[],
  onLost: () => void,
): Wall | null {
  const gl = canvas.getContext('webgl2', {
    alpha: true,
    antialias: true,
    premultipliedAlpha: true,
  });
  if (!gl || gl.isContextLost()) return null;
  const program = gl.createProgram();
  for (const [kind, text] of [
    [gl.VERTEX_SHADER, VERTEX],
    [gl.FRAGMENT_SHADER, FRAGMENT],
  ] as const) {
    const shader = gl.createShader(kind);
    if (!shader) return null;
    gl.shaderSource(shader, text);
    gl.compileShader(shader);
    gl.attachShader(program, shader);
  }
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) return null;
  // biome-ignore lint/correctness/useHookAtTopLevel: a WebGL call, not a React hook.
  gl.useProgram(program);
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

  const quad = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, quad);
  gl.bufferData(
    gl.ARRAY_BUFFER,
    new Float32Array([0, 0, 1, 0, 0, 1, 1, 1]),
    gl.STATIC_DRAW,
  );
  const corner = gl.getAttribLocation(program, 'corner');
  gl.enableVertexAttribArray(corner);
  gl.vertexAttribPointer(corner, 2, gl.FLOAT, false, 0, 0);

  const buffer = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
  const stride = 9 * 4;
  const attr = (name: string, size: number, offset: number) => {
    const at = gl.getAttribLocation(program, name);
    gl.enableVertexAttribArray(at);
    gl.vertexAttribPointer(at, size, gl.FLOAT, false, stride, offset);
    gl.vertexAttribDivisor(at, 1);
  };
  attr('rect', 4, 0);
  attr('meta', 3, 16);
  attr('shift', 2, 28);

  const u = (name: string) => gl.getUniformLocation(program, name);
  const [uScale, uWave, uDetail, uSorted, uAspect] = [
    'scale',
    'wave',
    'detail',
    'sorted',
    'aspect',
  ].map(u);
  gl.uniform1f(u('cols'), cols);
  // Anyone who asked for less motion still gets the finds, without the flashes.
  gl.uniform1f(
    u('flashes'),
    matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 1,
  );
  gl.uniform3fv(
    u('palette'),
    new Float32Array([0, 0, 0, ...colors.flatMap(hex)]),
  );
  const count = data.length / 9;

  let last: [number, number, number] = [1, 0, 0];
  const draw = (scale: number, wave: number, sorted: number) => {
    last = [scale, wave, sorted];
    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.uniform1f(uScale, scale);
    gl.uniform1f(uWave, wave);
    gl.uniform1f(uSorted, sorted);
    gl.uniform1f(uAspect, canvas.clientWidth / canvas.clientHeight);
    gl.uniform1f(
      uDetail,
      Math.min(1, Math.max(0, (canvas.clientHeight * scale - 14) / 26)),
    );
    gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, count);
  };
  const fit = () => {
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    canvas.width = canvas.clientWidth * dpr;
    canvas.height = canvas.clientHeight * dpr;
    draw(...last);
  };
  fit();
  const size = new ResizeObserver(fit);
  size.observe(canvas);
  // Phones drop the GPU's memory when the tab sleeps, so a lost context falls back too.
  canvas.addEventListener('webglcontextlost', onLost);
  return {
    draw,
    stop: () => {
      canvas.removeEventListener('webglcontextlost', onLost);
      size.disconnect();
    },
  };
}

/** The finished wall for browsers without WebGL2. It's drawn once, and scrolling doesn't change it. */
function FlatWall({
  colors,
  cols,
  finds,
  height,
  places,
}: {
  colors: string[];
  cols: number;
  finds: Find[];
  height: string;
  places: Map<number, [number, number]>;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const c = canvas.current;
    const g = c?.getContext('2d');
    if (!c || !g) return;
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    c.width = c.clientWidth * dpr;
    c.height = c.clientHeight * dpr;
    const w = c.width / cols;
    const h = c.height / cols;
    const kindOf = new Map(finds.map((f) => [f.id, f.kind]));
    for (let id = 0; id < cols * cols; id++) {
      const kind = kindOf.get(id);
      const [dx, dy] = places.get(id) ?? [0, 0];
      g.fillStyle = kind === undefined ? '#caaaf2' : colors[kind];
      g.globalAlpha = kind === undefined ? 0.28 : 1;
      g.fillRect(
        ((id % cols) + dx) * w + w * 0.08,
        (Math.floor(id / cols) + dy) * h + h * 0.08,
        w * 0.84,
        h * 0.84,
      );
    }
  }, [colors, cols, finds, places]);
  return <canvas className={`block w-full ${height}`} ref={canvas} />;
}

const WALL_H =
  'h-[clamp(280px,calc(100vh-400px),520px)] sm:h-[clamp(320px,calc(100vh-300px),520px)]';

function DataWall({ p }: { p: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const wall = useRef<Wall | null>(null);
  const [cols, setCols] = useState(WIDE_COLS);
  const [flat, setFlat] = useState(false);
  const finds = useMemo(() => findsOf(cols), [cols]);
  const places = useMemo(() => sortedPlaces(cols, finds), [cols, finds]);

  // Without the GPU there is no zoom, so the words go straight to the end too.
  const shown = flat ? 1 : p;
  const zoom = ramp(shown, 0.06, 0.48);
  const wave = ramp(shown, 0.52, 0.8) * (1 + POP);
  const sorted = ramp(shown, 0.84, 0.98);
  // A steady camera move, so the wall never has to be laid out again.
  const scale = cols ** -zoom;

  useEffect(() => {
    const c = canvas.current;
    if (!c) return;
    const count = c.clientWidth < 600 ? PHONE_COLS : WIDE_COLS;
    setCols(count);
    const noGpu = new URLSearchParams(window.location.search).has('flat');
    const f = findsOf(count);
    const data = wallInstances(count, f, sortedPlaces(count, f));
    const started = noGpu
      ? null
      : startWall(c, count, data, COLORS, () => setFlat(true));
    if (!started) setFlat(true);
    wall.current = started;
    return () => started?.stop();
  }, []);

  useEffect(() => {
    wall.current?.draw(scale, wave, sorted);
  }, [scale, wave, sorted]);

  // Past the first few, each box stands for a batch of runs.
  const runs = Math.round(Math.exp(zoom * Math.log(MAX_RUNS)));
  const hits = finds.filter((f) => f.at < wave);
  const perBox = MAX_RUNS / (cols * cols);
  const says =
    wave > 0
      ? 'An agent can process every one.'
      : runs < 3
        ? 'One run. You can read this.'
        : runs < 1000
          ? 'A few hundred. You can skim this.'
          : 'A month of runs. Nobody can read this.';
  return (
    <div>
      <div className="flex flex-col sm:flex-row sm:items-baseline justify-between gap-x-4 gap-y-1">
        <p className="v4-h3 text-head min-h-[2.5em] sm:min-h-0">{says}</p>
        <p className="font-mono text-[14px] tabular-nums text-white/60 sm:shrink-0">
          {compact.format(runs)} {runs === 1 ? 'run' : 'runs'},{' '}
          {compact.format(runs * SPANS_PER_RUN)} spans
          {hits.length > 0 && (
            <span style={{ color: '#fff' }}>
              , {compact.format(hits.length * perBox)} flagged
            </span>
          )}
        </p>
      </div>
      <div className="mt-4 rounded-xl overflow-hidden border border-rule bg-raised">
        <div className="relative">
          {flat ? (
            <FlatWall
              colors={COLORS}
              cols={cols}
              finds={finds}
              height={WALL_H}
              places={places}
            />
          ) : (
            <canvas className={`block w-full ${WALL_H}`} ref={canvas} />
          )}
          {zoom > 0 && sorted === 0 && (
            <span
              className="absolute left-0 top-0 border border-white"
              style={{ height: `${scale * 100}%`, width: `${scale * 100}%` }}
            />
          )}
        </div>
      </div>
      <div className="mt-3 grid grid-cols-2 md:grid-cols-4 gap-x-6 gap-y-1">
        {KINDS.map((k, i) => (
          <p
            className="flex items-center gap-2 text-[15px] text-white"
            key={k.name}
          >
            <span
              className="size-2.5 rounded-[2px]"
              style={{ background: k.color }}
            />
            {k.name}
            <span className="ml-auto font-mono text-[13px] tabular-nums text-white/60">
              {compact.format(hits.filter((f) => f.kind === i).length * perBox)}
            </span>
          </p>
        ))}
      </div>
    </div>
  );
}

export default function Agents() {
  return (
    <section
      {...section('agents')}
      aria-labelledby="agents"
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        <h2 className="v4-h2 text-head max-w-[22em]" id="agents">
          No person could read this much data. Agents can, and they need to.
        </h2>
        <p className="mt-4 max-w-[40rem] v4-body text-ink-2">
          Automated systems are only as good as their data. Give an agent gaps
          and it fills them with assumptions. With <Hyper />, there are no gaps.
        </p>
        <div className="mt-10 md:mt-12">
          <Pinned length="240vh" phone>
            {(p) => <DataWall p={p} />}
          </Pinned>
        </div>
      </div>
    </section>
  );
}
