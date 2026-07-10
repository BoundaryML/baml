'use client';

import { useEffect, useRef, useState } from 'react';

/*
 * Design-philosophy accordion. Six tenets; three of them open to bespoke
 * animations driven by imperative controllers (makeTraceAnim / makeBranchAnim /
 * makeLocalAnim). Ported faithfully from the standalone mockup: the controllers
 * stay imperative and operate on a demo's DOM subtree via a ref, rather than
 * being rewritten in React idiom, to minimize behavioral drift. React only owns
 * the empty scaffolds; each controller fills them.
 *
 * A tenet's animation runs ~300ms after it opens, stops on close / switch / and
 * on unmount, and renders its static end-state under prefers-reduced-motion.
 */

type Ctrl = { run: () => void; stop: () => void; showStatic: () => void };

const SVGNS = 'http://www.w3.org/2000/svg';

type CodeTok = { t: string; c?: string };
type DemoKind = 'trace' | 'branch' | 'local';
type Tenet = {
  title: string;
  why: string;
  code?: CodeTok[];
  demo?: DemoKind;
};

const TENETS: Tenet[] = [
  {
    title: 'Invent as little as necessary.',
    why: 'The more we invent, the worse agents are at it. Everything different from what they already know should be a deliberate decision.',
  },
  {
    code: [
      { c: 'cm', t: '// the model reply is just a string' },
      { t: '\n' },
      { c: 'bad', t: 'const user = reply as unknown as User' },
      { t: '\n' },
      { c: 'cm', t: '//   "trust me": compiles, checks nothing, breaks later' },
      { t: '\n\n' },
      {
        c: 'cm',
        t: '// BAML parses and validates the output, so there is no cast to write',
      },
      { t: '\n' },
      { c: 'ok', t: 'function ExtractUser(text: string) -> User' },
    ],
    title: 'Read like TypeScript, without the footguns.',
    why: 'Agents love TypeScript, and humans do too. Types, unions, generics, give me more. But TypeScript is bandaging up broken JavaScript, so it has real escape hatches agents love to abuse.',
  },
  {
    code: [
      { t: 'class Ticket {\n  status "open" | "closed"  ' },
      { c: 'cm', t: '// a typo like "opne" will not compile' },
      { t: '\n}' },
    ],
    title: 'Make undesired state unrepresentable.',
    why: "An agent sampling tokens will eventually write an invalid state. If it can't compile, it can't ship, and no human has to catch it.",
  },
  {
    demo: 'trace',
    title: 'Trace nondeterminism.',
    why: "As much as it hurts us: today is the most code you'll ever read. The only way to understand it will be through hindsight and focused traces.",
  },
  {
    demo: 'branch',
    title: 'Leave one obvious way.',
    why: 'Every option on the table eventually gets used by some agent. The codebase ends up carrying five versions of the same thing, and the next agent adds a sixth.',
  },
  {
    demo: 'local',
    title: 'Keep edits local.',
    why: "When a change isn't local, the agent goes on a side quest to chase it across the codebase. It comes back drifted, its context polluted with files that have nothing to do with the task.",
  },
  {
    title: 'Build tools for agents, not just IDEs.',
    why: "Humans still need their IDEs. But most code is now read and written by agents, and an agent can't hover, click, or read a tooltip.",
  },
];

const JUNK: [string, string][] = [
  ['apps/web/checkout.ts', 'from "../../lib/parse"'],
  ['apps/admin/orders.tsx', 'from "../../lib/parse"'],
  ['services/billing/tax.ts', 'from "../../../lib/parse"'],
  ['services/mail/receipt.ts', 'from "../../../lib/parse"'],
  ['jobs/nightly.ts', 'from "../../lib/parse"'],
  ['packages/core/ledger.ts', 'from "../../lib/parse"'],
  ['tests/parse.e2e.ts', 'from "../lib/parse"'],
  ['legacy/v1.js', 'require("../lib/parse")'],
];

// ==== tenet #5: keep edits local ====
function makeLocalAnim(root: HTMLElement): Ctrl {
  const MAXTOK = 64000;
  const fmtTok = (n: number) =>
    n >= 10000
      ? `${Math.round(n / 1000)}k tok`
      : `${(n / 1000).toFixed(1)}k tok`;
  const css = (v: string) => getComputedStyle(root).getPropertyValue(v).trim();
  let timers: ReturnType<typeof setTimeout>[] = [];
  const at = (ms: number, fn: () => void) => timers.push(setTimeout(fn, ms));
  const byId = (id: string) => root.querySelector(`#${id}`) as HTMLElement;
  function panel(sfx: string) {
    const stack = byId(`s${sfx}`);
    const fill = byId(`f${sfx}`);
    const pct = byId(`pct${sfx}`);
    const cap = byId(`c${sfx}`);
    const box = byId(`p${sfx}`);
    return {
      add(cls: string, file: string, note: string) {
        const d = document.createElement('div');
        d.className = `row ${cls}`;
        d.innerHTML = `<span class="dot"></span><span class="file">${file}</span><span class="note">${note}</span>`;
        stack.appendChild(d);
        return d;
      },
      mark(kind: string) {
        box.className = `panel ${kind}`;
      },
      meter(tok: number, color: string) {
        fill.style.width = `${Math.min(100, (tok / MAXTOK) * 100)}%`;
        fill.style.background = css(color);
        pct.textContent = fmtTok(tok);
        pct.style.color = color === '--soft' ? css('--soft') : css(color);
      },
      reset() {
        stack.innerHTML = '';
        cap.innerHTML = '';
        box.className = 'panel';
      },
      step(kind: string, text: string) {
        for (const s of cap.querySelectorAll('.step')) {
          s.classList.remove('is-active');
        }
        if (cap.children.length) {
          const a = document.createElement('span');
          a.className = 'tl-arrow';
          a.textContent = '→';
          cap.appendChild(a);
        }
        const s = document.createElement('span');
        s.className = `step step--${kind} is-active`;
        s.innerHTML = `<span class="sdot"></span>${text}`;
        cap.appendChild(s);
      },
    };
  }
  const L = panel('L');
  const R = panel('R');
  function stop() {
    for (const t of timers) clearTimeout(t);
    timers = [];
  }
  function run() {
    stop();
    L.reset();
    R.reset();
    L.meter(1200, '--accent');
    R.meter(1200, '--accent');
    const lt = L.add('row--task', 'lib/parse.ts', 'export parseInvoice()');
    L.add('row--call', 'checkout.ts', 'parseInvoice(order)');
    L.step('task', 'on task');
    const rt = R.add('row--task', 'lib/parse.ts', 'export parseInvoice()');
    R.step('task', 'on task');
    at(1100, () => {
      lt.className = 'row row--edit';
      (lt.querySelector('.file') as HTMLElement).textContent =
        'billing/parse.ts';
      L.step('task', 'moved');
      L.meter(1500, '--accent');
      rt.className = 'row row--edit';
      (rt.querySelector('.file') as HTMLElement).textContent =
        'billing/parse.ts';
      R.step('task', 'moved');
    });
    at(2000, () => {
      L.step('task', 'next task');
      L.add('row--edit', 'billing/parse.ts', 'export discount()');
      L.meter(2100, '--accent');
      L.mark('good');
    });
    at(1700, () => R.step('warn', 'side quest'));
    JUNK.forEach(([file, note], i) => {
      at(1900 + i * 300, () => {
        R.add('row--junk', file, note);
        const tok = 1500 + (i + 1) * 4800;
        R.meter(tok, tok > 20000 ? '--amber' : '--accent');
        if (i === 1) {
          for (const r of root.querySelectorAll(
            '#sR .row--edit, #sR .row--task',
          )) {
            r.classList.add('evicted');
          }
        }
      });
    });
    const floodEnd = 1900 + JUNK.length * 300;
    at(floodEnd + 300, () => {
      R.meter(39600, '--amber');
      R.mark('bad');
    });
    at(floodEnd + 1100, () => {
      R.step('task', 'next task');
      R.add('row--task', 'billing/parse.ts', 'export discount()');
      R.meter(44400, '--amber');
    });
    at(floodEnd + 1700, () => {
      R.add('row--junk', 'apps/web/checkout.ts', 're-read, path still wrong');
      R.add('row--junk', 'apps/admin/orders.tsx', 're-read, path still wrong');
      R.meter(54000, '--amber');
    });
    at(floodEnd + 4600, run);
  }
  function showStatic() {
    stop();
    L.reset();
    R.reset();
    L.add('row--edit', 'billing/parse.ts', 'export parseInvoice()');
    L.add('row--call', 'checkout.ts', 'parseInvoice(order)');
    L.add('row--edit', 'billing/parse.ts', 'export discount()');
    L.meter(2100, '--accent');
    L.mark('good');
    L.step('task', 'on task');
    L.step('task', 'moved');
    L.step('task', 'next task');
    R.add('row--task evicted', 'lib/parse.ts', 'export parseInvoice()');
    for (const [f, n] of JUNK) R.add('row--junk', f, n);
    R.add('row--task', 'billing/parse.ts', 'export discount()');
    R.meter(54000, '--amber');
    R.mark('bad');
    R.step('task', 'on task');
    R.step('task', 'moved');
    R.step('warn', 'side quest');
    R.step('task', 'next task');
  }
  return { run, showStatic, stop };
}

// ==== tenet #4: leave one obvious way ====
function makeBranchAnim(root: HTMLElement): Ctrl {
  const DEPTH = 4;
  let timers: ReturnType<typeof setTimeout>[] = [];
  const at = (ms: number, fn: () => void) => timers.push(setTimeout(fn, ms));
  const byId = (id: string) => root.querySelector(`#${id}`) as HTMLElement;
  const mkLine = (
    x1: number,
    y1: number,
    x2: number,
    y2: number,
    cls: string,
  ) => {
    const l = document.createElementNS(SVGNS, 'line');
    l.setAttribute('x1', String(x1));
    l.setAttribute('y1', String(y1));
    l.setAttribute('x2', String(x2));
    l.setAttribute('y2', String(y2));
    l.setAttribute('class', cls);
    return l;
  };
  const mkDot = (cls: string, x: number, y: number) => {
    const d = document.createElement('div');
    d.className = `tnode ${cls}`;
    d.style.left = `${x}%`;
    d.style.top = `${y}%`;
    return d;
  };
  function build(el: HTMLElement, mode: 'many' | 'one') {
    el.innerHTML = '';
    const svg = document.createElementNS(SVGNS, 'svg');
    svg.setAttribute('viewBox', '0 0 100 100');
    svg.setAttribute('preserveAspectRatio', 'none');
    el.appendChild(svg);
    const lnCls = mode === 'many' ? 'ln-many' : 'ln-one';
    const yOf = (lv: number) => 8 + lv * (74 / (DEPTH - 1));
    const levels: number[][] = [];
    for (let lv = 0; lv < DEPTH; lv++) {
      const n = mode === 'many' ? 2 ** lv : 1;
      const xs: number[] = [];
      for (let j = 0; j < n; j++) {
        xs.push(mode === 'many' ? 6 + ((j + 0.5) / n) * 88 : 50);
      }
      levels.push(xs);
    }
    const lineTiers: SVGLineElement[][] = [];
    for (let lv = 0; lv < DEPTH - 1; lv++) {
      const tier: SVGLineElement[] = [];
      levels[lv].forEach((px, j) => {
        const kids = mode === 'many' ? [2 * j, 2 * j + 1] : [0];
        for (const ci of kids) {
          const l = mkLine(px, yOf(lv), levels[lv + 1][ci], yOf(lv + 1), lnCls);
          svg.appendChild(l);
          tier.push(l);
        }
      });
      lineTiers.push(tier);
    }
    const nodeTiers = levels.map((xs, lv) =>
      xs.map((x) => {
        const d = mkDot(
          mode === 'many' ? 'tnode--many' : 'tnode--one',
          x,
          yOf(lv),
        );
        el.appendChild(d);
        return d;
      }),
    );
    return { lineTiers, nodeTiers };
  }
  const many = build(byId('bMany'), 'many');
  const one = build(byId('bOne'), 'one');
  const show = (arr: Element[]) => {
    for (const e of arr) e.classList.add('in');
  };
  function stop() {
    for (const t of timers) clearTimeout(t);
    timers = [];
  }
  function reset() {
    for (const g of [many, one]) {
      for (const e of g.nodeTiers.flat()) e.classList.remove('in');
      for (const e of g.lineTiers.flat()) e.classList.remove('in');
    }
  }
  function run() {
    stop();
    reset();
    at(200, () => {
      show(many.nodeTiers[0]);
      show(one.nodeTiers[0]);
    });
    for (let lv = 0; lv < DEPTH - 1; lv++) {
      const t = 700 + lv * 620;
      at(t, () => {
        show(many.lineTiers[lv]);
        show(one.lineTiers[lv]);
      });
      at(t + 260, () => {
        show(many.nodeTiers[lv + 1]);
        show(one.nodeTiers[lv + 1]);
      });
    }
    at(700 + (DEPTH - 1) * 620 + 2000, run);
  }
  function showStatic() {
    stop();
    reset();
    for (const g of [many, one]) {
      for (const e of g.nodeTiers.flat()) e.classList.add('in');
      for (const e of g.lineTiers.flat()) e.classList.add('in');
    }
  }
  return { run, showStatic, stop };
}

// ==== tenet #3: trace nondeterminism ====
type TraceNode = {
  name: string;
  cat: string;
  dur: number;
  note?: string;
  fail?: boolean;
  children?: TraceNode[];
};
type TraceSpan = TraceNode & { start: number; depth: number };

function makeTraceAnim(root: HTMLElement): Ctrl {
  const COLOR: Record<string, string> = {
    app: '#8b5cf6',
    db: '#2f9e8f',
    net: '#c2853a',
    retry: '#c2410c',
    root: '#8a8580',
  };
  const HAPPY: TraceNode = {
    cat: 'root',
    children: [
      {
        cat: 'app',
        children: [{ cat: 'app', dur: 2.3, name: 'jwt.verify' }],
        dur: 3.1,
        name: 'authenticate',
      },
      {
        cat: 'app',
        children: [
          { cat: 'db', dur: 9.6, name: 'db.query', note: 'select · carts' },
        ],
        dur: 11.4,
        name: 'Cart.load',
      },
      {
        cat: 'app',
        children: [
          {
            cat: 'net',
            dur: 21.4,
            name: 'stripe.paymentIntents.create',
          },
        ],
        dur: 24.9,
        name: 'Payment.charge',
      },
      {
        cat: 'app',
        children: [{ cat: 'db', dur: 5.0, name: 'db.insert', note: 'orders' }],
        dur: 6.7,
        name: 'Order.create',
      },
      { cat: 'app', dur: 0.8, name: 'email.enqueue' },
    ],
    dur: 48.6,
    name: 'POST /checkout',
  };
  const RETRY: TraceNode = {
    cat: 'root',
    children: [
      {
        cat: 'app',
        children: [{ cat: 'app', dur: 2.2, name: 'jwt.verify' }],
        dur: 2.9,
        name: 'authenticate',
      },
      {
        cat: 'app',
        children: [
          { cat: 'db', dur: 10.3, name: 'db.query', note: 'select · carts' },
        ],
        dur: 12.1,
        name: 'Cart.load',
      },
      {
        cat: 'app',
        children: [
          {
            cat: 'net',
            dur: 8.0,
            fail: true,
            name: 'stripe.paymentIntents.create',
            note: 'timeout',
          },
          {
            cat: 'retry',
            dur: 19.8,
            name: 'stripe.paymentIntents.create',
            note: 'retry',
          },
        ],
        dur: 38.2,
        name: 'Payment.charge',
      },
      {
        cat: 'app',
        children: [{ cat: 'db', dur: 4.8, name: 'db.insert', note: 'orders' }],
        dur: 6.4,
        name: 'Order.create',
      },
      { cat: 'app', dur: 0.9, name: 'email.enqueue' },
    ],
    dur: 61.9,
    name: 'POST /checkout',
  };
  const byId = (id: string) => root.querySelector(`#${id}`) as HTMLElement;
  const titleEl = byId('tvTitle');
  const gridEl = byId('tvGrid');
  const rowsEl = byId('tvRows');
  let timers: ReturnType<typeof setTimeout>[] = [];
  let variant = 0;
  let playhead: HTMLElement | null = null;
  const at = (ms: number, fn: () => void) => timers.push(setTimeout(fn, ms));
  function schedule(
    node: TraceNode,
    start: number,
    depth: number,
    out: TraceSpan[],
  ) {
    out.push({ ...node, depth, start });
    let cx = start;
    for (const c of node.children ?? []) {
      schedule(c, cx, depth + 1, out);
      cx += c.dur;
    }
    return out;
  }
  function render(trace: TraceNode) {
    const spans = schedule(trace, 0, 0, []);
    const total = trace.dur;
    const [name, path] = trace.name.split(' ');
    titleEl.innerHTML = `<span class="m">${name}</span> ${path}`;
    gridEl.innerHTML = '';
    for (let t = 0; t <= total; t += 15) {
      const gl = document.createElement('div');
      gl.className = 'tv-gl';
      gl.style.left = `${(t / total) * 100}%`;
      gridEl.appendChild(gl);
    }
    playhead = document.createElement('div');
    playhead.className = 'tv-playhead';
    gridEl.appendChild(playhead);
    rowsEl.innerHTML = '';
    return spans.map((s) => {
      const row = document.createElement('div');
      row.className = 'tv-row';
      const noteCls = s.fail ? 'fail' : 'note';
      row.innerHTML =
        `<div class="tv-label" style="padding-left:${14 + s.depth * 13}px">${s.name}${s.note ? ` <span class="${noteCls}">${s.note}</span>` : ''}</div>` +
        `<div class="tv-track"><div class="tv-bar${s.fail ? ' tv-bar--fail' : ''}" style="left:${(s.start / total) * 100}%;background:${COLOR[s.cat]}"></div></div>`;
      rowsEl.appendChild(row);
      return {
        bar: row.querySelector('.tv-bar') as HTMLElement,
        dur: s.dur,
        row,
        start: s.start,
        wPct: Math.max(1.2, (s.dur / total) * 100),
      };
    });
  }
  function stop() {
    for (const t of timers) clearTimeout(t);
    timers = [];
  }
  function run() {
    stop();
    const trace = variant % 2 === 0 ? HAPPY : RETRY;
    variant++;
    const rows = render(trace);
    const total = trace.dur;
    const ANIM = total * 46;
    const ph = playhead as HTMLElement;
    ph.style.transition = 'none';
    ph.style.left = '0';
    ph.classList.remove('on');
    requestAnimationFrame(() => {
      ph.classList.add('on');
      ph.style.transition = `left ${ANIM}ms linear`;
      ph.style.left = '100%';
      for (const r of rows) {
        at((r.start / total) * ANIM, () => {
          r.row.classList.add('in');
          r.bar.classList.add('on');
          r.bar.style.transition = `width ${(r.dur / total) * ANIM}ms linear`;
          r.bar.style.width = `${r.wPct}%`;
        });
      }
    });
    at(ANIM + 200, () => ph.classList.remove('on'));
    at(ANIM + 2400, run);
  }
  function showStatic() {
    stop();
    for (const r of render(HAPPY)) {
      r.row.classList.add('in');
      r.bar.classList.add('on');
      r.bar.style.width = `${r.wPct}%`;
    }
  }
  return { run, showStatic, stop };
}

function Chevron() {
  return (
    <svg
      aria-hidden="true"
      className="acc-chev"
      fill="none"
      viewBox="0 0 12 12"
    >
      <path
        d="M4.5 2.5 L8 6 L4.5 9.5"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1.4"
      />
    </svg>
  );
}

function TraceDemo({
  demoRef,
}: {
  demoRef: React.RefObject<HTMLDivElement | null>;
}) {
  return (
    <div className="tv" ref={demoRef}>
      <div className="tv-head">
        <span className="tv-title" id="tvTitle" />
      </div>
      <div className="tv-body">
        <div className="tv-grid" id="tvGrid" />
        <div id="tvRows" />
      </div>
    </div>
  );
}

function BranchDemo({
  demoRef,
}: {
  demoRef: React.RefObject<HTMLDivElement | null>;
}) {
  return (
    <div className="cols" ref={demoRef}>
      <div className="panel">
        <div className="p-head">
          <span className="p-eyebrow p-eyebrow--many">Many ways</span>
        </div>
        <div className="branch" id="bMany" />
      </div>
      <div className="panel">
        <div className="p-head">
          <span className="p-eyebrow p-eyebrow--one">One way</span>
        </div>
        <div className="branch" id="bOne" />
      </div>
    </div>
  );
}

function LocalPanel({
  sfx,
  eyebrow,
  eyebrowMod,
  title,
}: {
  sfx: string;
  eyebrow: string;
  eyebrowMod: string;
  title: string;
}) {
  return (
    <div className="panel" id={`p${sfx}`}>
      <div className="p-head">
        <div className="p-titlegroup">
          <span className={`p-eyebrow p-eyebrow--${eyebrowMod}`}>
            {eyebrow}
          </span>
          <span className="p-title">{title}</span>
        </div>
        <div className="meter">
          <div className="meter-track">
            <div className="meter-fill" id={`f${sfx}`} />
          </div>
          <span className="meter-pct" id={`pct${sfx}`}>
            1.2k tok
          </span>
        </div>
      </div>
      <div className="window">
        <div className="stack" id={`s${sfx}`} />
      </div>
      <div className="tl" id={`c${sfx}`} />
    </div>
  );
}

function LocalDemo({
  demoRef,
}: {
  demoRef: React.RefObject<HTMLDivElement | null>;
}) {
  return (
    <div className="cols" ref={demoRef}>
      <LocalPanel
        eyebrow="Non-local"
        eyebrowMod="nonlocal"
        sfx="R"
        title="Referenced by import"
      />
      <LocalPanel
        eyebrow="Local"
        eyebrowMod="local"
        sfx="L"
        title="Referenced by name"
      />
    </div>
  );
}

export function TenetsAccordion() {
  const [openIndex, setOpenIndex] = useState<number | null>(0);
  const traceRef = useRef<HTMLDivElement | null>(null);
  const branchRef = useRef<HTMLDivElement | null>(null);
  const localRef = useRef<HTMLDivElement | null>(null);
  const ctrls = useRef<{ trace?: Ctrl; branch?: Ctrl; local?: Ctrl }>({});

  // Build the imperative controllers once, after the scaffolds are mounted.
  useEffect(() => {
    if (traceRef.current) ctrls.current.trace = makeTraceAnim(traceRef.current);
    if (branchRef.current) {
      ctrls.current.branch = makeBranchAnim(branchRef.current);
    }
    if (localRef.current) ctrls.current.local = makeLocalAnim(localRef.current);
    const built = ctrls.current;
    return () => {
      for (const c of Object.values(built)) c?.stop();
    };
  }, []);

  // Play/stop as the open tenet changes: stop everything, then run the newly
  // open demo ~300ms later (or its static end-state under reduced motion).
  useEffect(() => {
    const all = ctrls.current;
    for (const c of Object.values(all)) c?.stop();
    const tenet = openIndex === null ? null : TENETS[openIndex];
    const demo = tenet?.demo;
    if (!demo) return;
    const ctrl = all[demo];
    if (!ctrl) return;
    const reduce = window.matchMedia(
      '(prefers-reduced-motion: reduce)',
    ).matches;
    const timer = setTimeout(() => {
      if (reduce) ctrl.showStatic();
      else ctrl.run();
    }, 300);
    return () => {
      clearTimeout(timer);
      ctrl.stop();
    };
  }, [openIndex]);

  const toggle = (i: number) => setOpenIndex((cur) => (cur === i ? null : i));

  return (
    <div className="l6-tenets-accordion">
      <ul className="acc">
        {TENETS.map((t, i) => {
          const open = openIndex === i;
          const pad = 'acc-body-pad';
          return (
            <li className={`acc-item${open ? ' is-open' : ''}`} key={t.title}>
              <button
                aria-expanded={open}
                className="acc-head"
                onClick={() => toggle(i)}
                type="button"
              >
                <span className="acc-title">{t.title}</span>
                <Chevron />
              </button>
              <div className="acc-body">
                <div className="acc-body-inner">
                  <div className={pad}>
                    <p className="why">{t.why}</p>
                    {t.code ? (
                      <pre className="code">
                        {t.code.map((tok, ti) =>
                          tok.c ? (
                            <span
                              className={tok.c}
                              // biome-ignore lint/suspicious/noArrayIndexKey: static, order-stable token list
                              key={ti}
                            >
                              {tok.t}
                            </span>
                          ) : (
                            tok.t
                          ),
                        )}
                      </pre>
                    ) : null}
                    {t.demo === 'trace' ? (
                      <>
                        <TraceDemo demoRef={traceRef} />
                        <button
                          className="replay"
                          onClick={() => ctrls.current.trace?.run()}
                          type="button"
                        >
                          ↻ replay
                        </button>
                      </>
                    ) : null}
                    {t.demo === 'branch' ? (
                      <>
                        <BranchDemo demoRef={branchRef} />
                        <button
                          className="replay"
                          onClick={() => ctrls.current.branch?.run()}
                          type="button"
                        >
                          ↻ replay
                        </button>
                      </>
                    ) : null}
                    {t.demo === 'local' ? (
                      <>
                        <LocalDemo demoRef={localRef} />
                        <button
                          className="replay"
                          onClick={() => ctrls.current.local?.run()}
                          type="button"
                        >
                          ↻ replay
                        </button>
                      </>
                    ) : null}
                  </div>
                </div>
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
