'use client';

import { useRef } from 'react';
import { section } from './analytics';
import Blueprints from './blueprints';
import { useScrollProgress } from './scroll';

/** Section 2: why BAML is a language, from the founders, beside a blueprint that changes as the reader
 *  scrolls: three Raptor engines, then the layers of the stack. On wide screens the text and drawing
 *  hold in the middle of the window while the drawing plays out. */
export default function Statement() {
  const track = useRef<HTMLDivElement>(null);
  const pin = useRef<HTMLDivElement>(null);
  const drawing = useRef<HTMLDivElement>(null);
  const [progress] = useScrollProgress(track, pin, drawing);
  return (
    <section
      {...section('why')}
      aria-labelledby="why"
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        {/* The heading runs across the whole section, over both the text and the drawing. */}
        <h2 className="v4-h2" id="why">
          Iteration loops are <em className="pr-[0.08em]">fast</em> when
          everything speaks the same language.
        </h2>
        <div className="mt-12 md:mt-16" ref={track}>
          <div
            className="grid lg:grid-cols-[minmax(0,1fr)_minmax(0,1.05fr)] gap-x-16 gap-y-12 items-center lg:motion-safe:sticky"
            ref={pin}
          >
            <div>
              <p className="font-display font-medium text-[24px] md:text-[30px] leading-[1.2] tracking-[-0.02em] text-balance text-head">
                We didn't want to build a programming language.
              </p>
              <div className="mt-6 flex flex-col gap-5 max-w-[36rem] v4-body text-ink-2">
                <p>
                  But to make something 100x better, you have to vertically
                  integrate. So{' '}
                  <strong className="font-semibold text-head">
                    we built BAML
                  </strong>
                  , with a virtual machine that makes it run anywhere,
                </p>
                <p>
                  a compiler that knows which errors you're hitting in
                  production,
                </p>
                <p>
                  and a data lake that stores every call in a structured way,
                  even with the types you declared.
                </p>
              </div>
            </div>
            <div ref={drawing}>
              <Blueprints progress={progress ?? 0} />
            </div>
          </div>
          {/* Scroll room for the drawing while the grid above is pinned. */}
          <div aria-hidden className="hidden lg:motion-safe:block h-[80vh]" />
        </div>
      </div>
    </section>
  );
}
