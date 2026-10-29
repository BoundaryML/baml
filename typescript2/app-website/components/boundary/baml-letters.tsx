/** biome-ignore-all lint/style/noNonNullAssertion: the ref and the letter paths it queries are rendered by this component */
'use client';

import { useEffect, useRef } from 'react';

const SRC = '/boundary/baml-letters.svg'; // one <path> per letter, each with data-k/data-x/data-y
const VIEW_W = 1792;
const RADIUS = 40; // svg units around the cursor that glitch
const FRAMES = 6; // swaps before a letter settles back
const FRAME_MS = 45;
const LILAC = '#CAAAF2';

type Letter = {
  el: SVGPathElement;
  d: string;
  k: string;
  x: number;
  y: number;
  cx: number;
  cy: number;
  busy: boolean;
};

/**
 * The scattered "BOUNDARY" letters behind the BAML section. Hovering near a letter makes it
 * and its neighbours flicker through other letters (in Clover) and then settle, like the
 * Geist Pixel words elsewhere on the site. The artwork loads only when the section is close.
 */
export default function BamlLetters() {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const host = ref.current!;
    const section = host.parentElement!;
    const reduced = window.matchMedia(
      '(prefers-reduced-motion: reduce)',
    ).matches;
    let letters: Letter[] = [];
    let svg: SVGSVGElement | null = null;
    let raf = 0;
    let last: PointerEvent | null = null;
    const timers = new Set<ReturnType<typeof setTimeout>>();
    let cancelled = false;

    const glitch = (L: Letter) => {
      L.busy = true;
      L.el.style.fill = LILAC;
      let n = 0;
      const step = () => {
        if (n++ < FRAMES) {
          let other = letters[Math.floor(Math.random() * letters.length)];
          for (let t = 0; t < 6 && other.k === L.k; t++)
            other = letters[Math.floor(Math.random() * letters.length)];
          L.el.setAttribute('d', other.d);
          L.el.setAttribute(
            'transform',
            `translate(${L.x - other.x} ${L.y - other.y})`,
          );
          const id = setTimeout(() => {
            timers.delete(id);
            step();
          }, FRAME_MS);
          timers.add(id);
        } else {
          L.el.setAttribute('d', L.d);
          L.el.removeAttribute('transform');
          L.el.style.fill = '';
          const id = setTimeout(() => {
            timers.delete(id);
            L.busy = false;
          }, 250); // short cooldown
          timers.add(id);
        }
      };
      step();
    };

    const update = () => {
      raf = 0;
      if (!svg || !last) return;
      const r = svg.getBoundingClientRect();
      const scale = VIEW_W / r.width;
      const px = (last.clientX - r.left) * scale;
      const py = (last.clientY - r.top) * scale;
      for (const L of letters) {
        if (L.busy) continue;
        const dx = L.cx - px;
        const dy = L.cy - py;
        if (dx * dx + dy * dy < RADIUS * RADIUS) glitch(L);
      }
    };
    const onMove = (e: PointerEvent) => {
      if (e.pointerType !== 'mouse') return;
      last = e;
      if (!raf) raf = requestAnimationFrame(update);
    };

    const load = async () => {
      const text = await fetch(SRC).then((r) => r.text());
      if (cancelled) return;
      host.innerHTML = text; // our own static asset
      svg = host.querySelector('svg');
      if (!svg) return;
      svg.setAttribute('aria-hidden', 'true');
      svg.style.width = '100%';
      svg.style.height = 'auto';
      svg.style.display = 'block';
      letters = [...svg.querySelectorAll<SVGPathElement>('path[data-k]')].map(
        (el) => {
          const b = el.getBBox();
          return {
            busy: false,
            cx: b.x + b.width / 2,
            cy: b.y + b.height / 2,
            d: el.getAttribute('d')!,
            el,
            k: el.dataset.k!,
            x: Number(el.dataset.x),
            y: Number(el.dataset.y),
          };
        },
      );
      if (!reduced) section.addEventListener('pointermove', onMove);
    };

    // fetch the artwork only once the section is near the viewport
    const io = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting) {
          io.disconnect();
          load();
        }
      },
      { rootMargin: '400px' },
    );
    io.observe(section);

    return () => {
      cancelled = true;
      io.disconnect();
      cancelAnimationFrame(raf);
      timers.forEach(clearTimeout);
      section.removeEventListener('pointermove', onMove);
    };
  }, []);

  return (
    <div
      aria-hidden
      className="absolute left-0 top-0 opacity-45 md:opacity-100 pointer-events-none select-none [&_path]:transition-[fill] [&_path]:duration-300"
      ref={ref}
      style={{ width: 'max(1792px, 100%)' }}
    />
  );
}
