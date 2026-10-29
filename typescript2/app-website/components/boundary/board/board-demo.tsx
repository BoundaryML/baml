/** biome-ignore-all lint/style/noNonNullAssertion: the refs and the SVG nodes the effect queries are rendered by this component, so they exist when it runs */
'use client';

import { useEffect, useRef } from 'react';
import { track } from '../track';
import { BOARD_DATA, VIEWS_HTML } from './views';
import './board.css';

type Theme = 'light' | 'dark';
type View = 'boards' | 'workspace' | 'execution';

/**
 * The Boundary "Invoice pipeline" board as a live product shot.
 * Loads in (bars grow, lines draw, numbers count) when it scrolls into view; hover for
 * tooltips, click legends, Refresh, a failure bar (opens the execution) or the app's tabs.
 */
export default function BoardDemo({
  initialTheme = 'light',
}: {
  initialTheme?: Theme;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const viewsRef = useRef<HTMLDivElement>(null);
  const tipRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const root = rootRef.current!;
    const views = viewsRef.current!;
    const tip = tipRef.current!;
    const reduce = window.matchMedia(
      '(prefers-reduced-motion: reduce)',
    ).matches;
    const state: { theme: Theme; view: View; prev: View; played: boolean } = {
      played: false,
      prev: 'boards',
      theme: initialTheme,
      view: 'boards',
    };
    // follow the site's light/dark theme (<html data-theme>)
    const siteTheme = (): Theme | null => {
      const t = document.documentElement.dataset.theme;
      return t === 'dark' || t === 'light' ? t : null;
    };
    state.theme = siteTheme() ?? initialTheme;
    let frames: number[] = [];
    let clock: ReturnType<typeof setInterval> | undefined;

    const svgFor = (theme: Theme, view: View) =>
      views.querySelector<SVGSVGElement>(
        `svg.view[data-theme="${theme}"][data-view="${view}"]`,
      )!;
    const board = () => svgFor(state.theme, 'boards');
    const ease = (t: number) => 1 - (1 - t) ** 3;
    const delayOf = (el: Element) => {
      const d = getComputedStyle(el).animationDelay.split(',')[0].trim();
      return d.endsWith('ms')
        ? Number.parseFloat(d)
        : Number.parseFloat(d) * 1000 || 0;
    };
    const cssVar = (n: string) =>
      getComputedStyle(root).getPropertyValue(n).trim();

    // ---- load animation ------------------------------------------------------------
    function countUp(scope: Element) {
      scope.querySelectorAll<SVGTSpanElement>('[data-to]').forEach((span) => {
        const to = Number.parseFloat(span.dataset.to!);
        const dec = Number(span.dataset.dec);
        const fmt = (v: number) => v.toFixed(dec);
        if (reduce) {
          span.textContent = fmt(to);
          return;
        }
        span.textContent = fmt(0);
        const start = performance.now() + delayOf(span.parentNode as Element);
        const step = (now: number) => {
          const t = Math.min(1, Math.max(0, (now - start) / 1000));
          span.textContent = fmt(to * ease(t));
          if (t < 1) frames.push(requestAnimationFrame(step));
        };
        frames.push(requestAnimationFrame(step));
      });
    }

    function refreshPanel(svg: SVGSVGElement, p: string) {
      const g = svg.querySelector<SVGGElement>(`.panel[data-p="${p}"]`)!;
      const els = [g, ...g.querySelectorAll<SVGElement>('*')];
      els.forEach((e) => {
        e.style.animation = 'none';
      });
      void root.offsetWidth;
      els.forEach((e) => {
        e.style.animation = '';
      });
      countUp(g);
      const t = svg.querySelector<SVGTSpanElement>(
        `[data-tick][data-panel="${p}"]`,
      );
      if (t) {
        t.dataset.now = '0';
        t.textContent = '0s';
      }
    }

    // "last completed Ns ago" keeps ticking; a panel re-runs when it reaches 60s, like the real board
    function startClock(svg: SVGSVGElement) {
      const spans = [...svg.querySelectorAll<SVGTSpanElement>('[data-tick]')];
      spans.forEach((s) => {
        s.dataset.now = s.dataset.tick;
        s.textContent = `${s.dataset.tick}s`;
      });
      clearInterval(clock);
      clock = setInterval(
        () =>
          spans.forEach((s) => {
            const n = Number(s.dataset.now) + 1;
            if (n >= 60) {
              refreshPanel(svg, s.dataset.panel!);
              return;
            }
            s.dataset.now = String(n);
            s.textContent = `${n}s`;
          }),
        1000,
      );
    }

    function play() {
      frames.forEach(cancelAnimationFrame);
      frames = [];
      const svg = board();
      svg.classList.remove('play', 'armed');
      void svg.getBoundingClientRect();
      svg.classList.add('play');
      countUp(svg);
      startClock(svg);
    }

    // ---- views & theme ------------------------------------------------------------------
    function show() {
      views.querySelectorAll('svg.view').forEach((v) => {
        const el = v as SVGSVGElement;
        const on =
          el.dataset.theme === state.theme && el.dataset.view === state.view;
        if (on) el.removeAttribute('hidden');
        else el.setAttribute('hidden', '');
      });
      root.dataset.board = state.theme;
      hideTip();
    }

    function go(view: View) {
      if (view === state.view) return;
      state.prev = state.view;
      state.view = view;
      show();
      if (view === 'boards') play();
    }

    function setTheme(theme: Theme) {
      state.theme = theme;
      show();
      if (state.view === 'boards') play();
    }

    // ---- tooltip & hover ------------------------------------------------------------------
    const sw = (c: string) => `<i style="background:${c}"></i>`;
    const row = (key: string, val: string | number, color?: string) =>
      `<div class="r"><span class="k">${color ? sw(color) : ''}${key}</span><span class="v">${val}</span></div>`;

    function placeTip(e: MouseEvent, html: string) {
      tip.innerHTML = html;
      tip.hidden = false;
      const r = root.getBoundingClientRect();
      let x = e.clientX - r.left + 16;
      let y = e.clientY - r.top + 16;
      if (x + tip.offsetWidth > r.width - 8)
        x = e.clientX - r.left - tip.offsetWidth - 16;
      if (y + tip.offsetHeight > r.height - 8)
        y = e.clientY - r.top - tip.offsetHeight - 16;
      tip.style.left = `${x}px`;
      tip.style.top = `${y}px`;
    }

    function hideTip() {
      tip.hidden = true;
      views.querySelectorAll('.hl').forEach((el) => el.classList.remove('hl'));
      views
        .querySelectorAll('.dim-bars, .rowfocus')
        .forEach((el) => el.classList.remove('dim-bars', 'rowfocus'));
      views.querySelectorAll<SVGGElement>('.xh').forEach((el) => {
        el.style.display = 'none';
      });
    }

    const toSvgX = (svg: SVGSVGElement, e: MouseEvent) => {
      const p = svg.createSVGPoint();
      p.x = e.clientX;
      p.y = e.clientY;
      return p.matrixTransform(svg.getScreenCTM()!.inverse()).x;
    };
    const nearest = <T extends { x: number }>(pts: readonly T[], x: number) =>
      pts.reduce((a, b) => (Math.abs(b.x - x) < Math.abs(a.x - x) ? b : a));
    const setAttrs = (el: Element | null, attrs: Record<string, number>) =>
      Object.entries(attrs).forEach(([k, v]) => el?.setAttribute(k, String(v)));

    function onMove(e: MouseEvent) {
      const target = e.target as Element;
      const hit = target.closest<SVGRectElement>('.hit');
      const svg = target.closest<SVGSVGElement>('svg.view');
      if (!hit || !svg || svg.dataset.view !== 'boards') {
        hideTip();
        return;
      }
      const kind = hit.dataset.kind;
      hideTip();

      if (kind === 'bar') {
        const i = Number(hit.dataset.i);
        const d = BOARD_DATA.bars[i];
        svg.classList.add('dim-bars');
        svg
          .querySelectorAll(`[data-m="${i}"]`)
          .forEach((el) => el.classList.add('hl'));
        hit.classList.toggle('clickable', d.bad > 0);
        placeTip(
          e,
          `<div class="h">${d.t}</div>` +
            row('succeeded', d.ok, cssVar('--grey')) +
            row('failed', d.bad, cssVar('--red')) +
            (d.bad
              ? `<div class="hint">Click to open a failed execution</div>`
              : ''),
        );
      } else if (kind === 'p95') {
        const p = nearest(BOARD_DATA.p95, toSvgX(svg, e));
        const xh = svg.querySelector<SVGGElement>('.xh[data-for="p95"]')!;
        xh.style.display = '';
        setAttrs(xh.querySelector('line'), { x1: p.x, x2: p.x });
        setAttrs(xh.querySelector('circle'), { cx: p.x, cy: p.y });
        placeTip(
          e,
          `<div class="h">${p.t}</div>` +
            row('p95', `${p.v.toFixed(2)}s`, cssVar('--accent')),
        );
      } else if (kind === 'fresh') {
        const p = nearest(BOARD_DATA.fresh, toSvgX(svg, e));
        const xh = svg.querySelector<SVGGElement>('.xh[data-for="fresh"]')!;
        xh.style.display = '';
        setAttrs(xh.querySelector('line'), { x1: p.x, x2: p.x });
        setAttrs(xh.querySelector('.xp'), { cx: p.x, cy: p.yp });
        setAttrs(xh.querySelector('.xs'), { cx: p.x, cy: p.ys });
        placeTip(
          e,
          `<div class="h">${p.t}</div>` +
            row('p50 seal → publish', `${p.vp}s`, cssVar('--accent')) +
            row('slowest batch', `${p.vs}s`, cssVar('--grey')),
        );
      } else if (kind === 'db' || kind === 'st') {
        const i = Number(hit.dataset.i);
        const panel = hit.closest('.panel')!;
        panel.classList.add('rowfocus');
        panel.querySelector(`.row[data-row="${i}"]`)?.classList.add('hl');
        if (kind === 'db') {
          const d = BOARD_DATA.db[i];
          placeTip(
            e,
            `<div class="h">${d.cc} · ExtractInvoice calls</div>` +
              row(
                'r41 · a1f3c9e',
                `${d.n1} of ${d.d1} · ${d.a}%`,
                cssVar('--grey'),
              ) +
              row(
                'r42 · 7be0d42',
                `${d.n2} of ${d.d2} · ${d.b}%`,
                cssVar('--accent'),
              ),
          );
        } else {
          const d = BOARD_DATA.st[i];
          placeTip(
            e,
            `<div class="h">${d.cc} · ${d.tot} validated invoices</div>` +
              row('accepted automatically', d.tot - d.need, cssVar('--grey')) +
              row(
                'needs manual review',
                `${d.need} · ${d.pct}%`,
                cssVar('--accent'),
              ),
          );
        }
      }
    }

    // ---- clicks ------------------------------------------------------------------------
    function onClick(e: MouseEvent) {
      const hit = (e.target as Element).closest<SVGRectElement>('.hit');
      if (!hit) return;
      const svg = hit.closest<SVGSVGElement>('svg.view')!;
      const kind = hit.dataset.kind;
      track('board_demo_clicked', {
        kind,
        target:
          hit.dataset.go ??
          hit.dataset.series ??
          hit.dataset.p ??
          hit.dataset.i,
        view: state.view,
      });
      if (kind === 'go') go(hit.dataset.go as View);
      else if (kind === 'back')
        go(state.prev === 'execution' ? 'boards' : state.prev);
      else if (kind === 'theme')
        setTheme(state.theme === 'light' ? 'dark' : 'light');
      else if (kind === 'refresh') refreshPanel(svg, hit.dataset.p!);
      else if (kind === 'legend') {
        const s = hit.dataset.series!;
        svg.classList.toggle(`off-${s}`);
        svg
          .querySelector(`.legend[data-series="${s}"]`)
          ?.classList.toggle('off');
      } else if (kind === 'bar' && BOARD_DATA.bars[Number(hit.dataset.i)].bad)
        go('execution');
    }

    root.addEventListener('mousemove', onMove);
    root.addEventListener('mouseleave', hideTip);
    root.addEventListener('click', onClick);
    show();

    // When the site theme flips, swap boards; replay only if it has already loaded in.
    const themeWatch = new MutationObserver(() => {
      const t = siteTheme();
      if (!t || t === state.theme) return;
      if (state.played) {
        setTheme(t);
        return;
      }
      state.theme = t;
      show();
      if (!reduce) board().classList.add('armed');
    });
    themeWatch.observe(document.documentElement, {
      attributeFilter: ['data-theme'],
      attributes: true,
    });

    // Wait at zero until the board is on screen, then load in once.
    if (!reduce) board().classList.add('armed');
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((en) => en.isIntersecting) && !state.played) {
          state.played = true;
          (document.fonts ? document.fonts.ready : Promise.resolve()).then(
            play,
          );
          io.disconnect();
        }
      },
      { threshold: 0.1 },
    ); // low enough for the phone crop, which only shows ~19% of the board
    io.observe(root);

    return () => {
      themeWatch.disconnect();
      io.disconnect();
      clearInterval(clock);
      frames.forEach(cancelAnimationFrame);
      root.removeEventListener('mousemove', onMove);
      root.removeEventListener('mouseleave', hideTip);
      root.removeEventListener('click', onClick);
    };
  }, [initialTheme]);

  return (
    <div className="board-demo" data-board={initialTheme} ref={rootRef}>
      {/* biome-ignore lint/security/noDangerouslySetInnerHtml: VIEWS_HTML is the board's static SVG markup from ./views.ts */}
      <div dangerouslySetInnerHTML={{ __html: VIEWS_HTML }} ref={viewsRef} />
      <div className="tip" hidden ref={tipRef} />
    </div>
  );
}
