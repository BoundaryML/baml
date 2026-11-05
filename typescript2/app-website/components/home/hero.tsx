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

/** The homepage hero: the headline, the subtitle and buttons, and the toolchain diagram under them. */
export default function Hero() {
  const [styleIndex, setStyleIndex] = useState(DEFAULT_STYLE);
  const style = DIAGRAM_STYLES[styleIndex];

  return (
    <section
      {...section('hero')}
      aria-labelledby="hero-title"
      className="relative"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) hero-top pb-10 md:pb-14">
        <h1 className="v4-h1 hero-title max-w-[13em]" id="hero-title">
          The <span className="whitespace-nowrap">hyper-observable</span>{' '}
          language
        </h1>
        <div className="flex flex-wrap items-end justify-between gap-x-12 gap-y-6 mt-5 md:mt-7">
          <p className="grow basis-[26rem] max-w-[36rem] v4-body text-ink-2">
            BAML is a programming language with built-in telemetry. Replace 10+
            separate tools with one toolchain that helps agents test, trace,
            query data, build alerts, and more.
          </p>
          <div className="flex flex-wrap gap-2.5">
            <Button href={BOOK}>Get early access</Button>
            <Button href="#how" variant="secondary">
              See how it works
            </Button>
          </div>
        </div>

        {/* The style button floats on the diagram's top edge when it's shown. */}
        <div className="relative hero-diagram">
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
