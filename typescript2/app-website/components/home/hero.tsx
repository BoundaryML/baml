'use client';

import { useState } from 'react';
import { section } from './analytics';
import { DIAGRAM_STYLES } from './hero-languages-styled';
import { BOOK } from './links';
import Tracker from './tracker';
import { Button } from './ui';

// Glass is the pick, without its shadows and glow (v4.css removes them on this page). The style
// button is hidden, not removed, in case we compare again: set SHOW_STYLES to bring it back.
const DEFAULT_STYLE = DIAGRAM_STYLES.findIndex((s) => s.id === 'glass');
const SHOW_STYLES = false;

export default function Hero() {
  const [styleIndex, setStyleIndex] = useState(DEFAULT_STYLE);
  const style = DIAGRAM_STYLES[styleIndex];

  return (
    <section
      {...section('hero')}
      aria-labelledby="hero-title"
      className="relative"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) pt-12 md:pt-20 pb-10 md:pb-14">
        <h1 className="v4-h1 max-w-[13em]" id="hero-title">
          The unified software stack for agents
        </h1>
        <div className="flex flex-wrap items-end justify-between gap-x-12 gap-y-6 mt-5 md:mt-7">
          <p className="max-w-[36rem] v4-body text-ink-2">
            BAML is a language with a vertically integrated toolchain that helps
            agents in production. Test and trace code, query data, build alerts,
            and replace 10+ separate tools.
          </p>
          <div className="flex flex-wrap gap-2.5">
            <Button href={BOOK}>Get early access</Button>
            <Button href="#how" variant="secondary">
              See how it works
            </Button>
          </div>
        </div>

        {/* The style button floats on the diagram's top edge when it's shown. */}
        <div className="relative mt-12 md:mt-16">
          {SHOW_STYLES && (
            <button
              className="focus-ring absolute right-5 md:right-8 top-0 z-10 -translate-y-1/2 rounded-full border border-rule bg-ground px-3 py-1.5 text-[13px] text-body hover:text-ink transition-colors"
              onClick={(e) =>
                setStyleIndex(
                  (i) =>
                    (i + (e.shiftKey ? -1 : 1) + DIAGRAM_STYLES.length) %
                    DIAGRAM_STYLES.length,
                )
              }
              type="button"
            >
              Style <span className="text-ink">{style.name}</span>
            </button>
          )}
          <Tracker style={style.id} />
        </div>
      </div>
    </section>
  );
}
