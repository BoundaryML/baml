'use client';

import { useState } from 'react';
import { section } from '../analytics';
import { Button } from '../ui';
import { DIAGRAM_STYLES } from '../v3/hero-languages-styled';
import { BOOK } from './links';
import Tracker from './tracker';

// The style button stays while we compare. Midnight is the pick so far.
const DEFAULT_STYLE = DIAGRAM_STYLES.findIndex((s) => s.id === 'midnight');

export default function Hero() {
  const [styleIndex, setStyleIndex] = useState(DEFAULT_STYLE);
  const style = DIAGRAM_STYLES[styleIndex];

  return (
    <section
      {...section('hero')}
      aria-labelledby="hero-title"
      className="relative"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) pt-12 md:pt-20 pb-16 md:pb-24">
        <h1 className="v3-h1 max-w-[13em] text-balance" id="hero-title">
          The unified software stack for agents
        </h1>
        <div className="flex flex-wrap items-end justify-between gap-x-12 gap-y-6 mt-5 md:mt-7">
          <p className="max-w-[34rem] text-base md:text-lg leading-[1.45] tracking-[-0.01em] text-white/70">
            Writing code is fast, but shipping still means wiring together 11
            tools that don't know about each other. BAML replaces them with one
            language and toolchain.
          </p>
          <div className="flex flex-wrap gap-2.5">
            <Button href={BOOK}>Get early access</Button>
            <Button href="#how" variant="secondary">
              See how it works
            </Button>
          </div>
        </div>

        <div className="mt-10 md:mt-14">
          <Tracker style={style.id} />
          <div className="flex justify-end mt-3">
            <button
              className="focus-ring rounded-full border border-white/12 px-3 py-1.5 text-[13px] text-white/55 hover:text-white/85 transition-colors"
              onClick={(e) =>
                setStyleIndex(
                  (i) =>
                    (i + (e.shiftKey ? -1 : 1) + DIAGRAM_STYLES.length) %
                    DIAGRAM_STYLES.length,
                )
              }
              type="button"
            >
              Style <span className="text-white/85">{style.name}</span>
            </button>
          </div>
        </div>
      </div>
    </section>
  );
}
