'use client';

import { useEffect, useRef, useState } from 'react';
import { RED } from './story';

// Half an hour in the life of one function, on five tracks that share a clock: source changes, builds,
// deployments, traces, and experiments. Because they line up, an edit, the build and deploy it caused,
// and the change it made in production read as one column. A playhead crosses slowly and lights each
// event as it passes. All data is example data.
const MINUTES = 30;
const START_HOUR = 14;
const TRACKS = [
  'Source changes',
  'Builds',
  'Deployments',
  'Traces',
  'Experiments',
];
const SOURCE = 0;
const BUILD = 1;
const DEPLOY = 2;
const TRACE = 3;
const EXPERIMENT = 4;

type Event = { at: number; label: string; log: string; track: number };

const EVENTS: Event[] = [
  {
    at: 2,
    label: 'Edited classify prompt',
    log: 'Edited the classify prompt',
    track: SOURCE,
  },
  {
    at: 3,
    label: 'Build 318 passed',
    log: 'Build 318 passed: baml check and 12 tests',
    track: BUILD,
  },
  { at: 5, label: 'v42', log: 'Deployed v42', track: DEPLOY },
  {
    at: 9,
    label: 'Errors 4.1% to 0.6%',
    log: 'Errors on classify fell from 4.1% to 0.6%',
    track: TRACE,
  },
  {
    at: 12,
    label: 'gpt-4o against gpt-4o-mini',
    log: 'Started an experiment: gpt-4o against gpt-4o-mini',
    track: EXPERIMENT,
  },
  {
    at: 18,
    label: 'Added test angry_refund',
    log: 'Added test angry_refund',
    track: SOURCE,
  },
  {
    at: 19,
    label: 'Build 319 passed',
    log: 'Build 319 passed: 13 tests',
    track: BUILD,
  },
  { at: 21, label: 'v43', log: 'Deployed v43', track: DEPLOY },
  {
    at: 26,
    label: 'gpt-4o-mini matched accuracy at 38% lower cost',
    log: 'Experiment ended: gpt-4o-mini matched accuracy at 38% lower cost',
    track: EXPERIMENT,
  },
];
const EXPERIMENT_START = 12;
const EXPERIMENT_END = 26;
const DEPLOYED = 5;

// One trace bar every 15 seconds. Errors are more common before v42 than after it.
const BARS = Array.from({ length: MINUTES * 4 }, (_, i) => {
  const at = i / 4;
  const r = Math.abs(Math.sin(i * 12.9898) * 43758.5453) % 1;
  const h = Math.abs(Math.sin(i * 78.233) * 12345.678) % 1;
  return { at, error: r < (at < DEPLOYED ? 0.2 : 0.025), height: 30 + h * 60 };
});

const LAP_MS = 28000;
const HOLD_MS = 3500;

const clock = (m: number) =>
  `${START_HOUR}:${String(Math.floor(m)).padStart(2, '0')}`;
const x = (m: number) => `${(m / MINUTES) * 100}%`;

/** The error rate on classify at a minute: high until the deploy, then falling to its new level. */
function errorRate(m: number) {
  if (m < DEPLOYED) return 4.1;
  if (m > 9) return 0.6;
  return 4.1 + ((m - DEPLOYED) / (9 - DEPLOYED)) * (0.6 - 4.1);
}

function Marker({
  event,
  on,
  latest,
}: {
  event: Event;
  on: boolean;
  latest: boolean;
}) {
  const deploy = event.track === DEPLOY;
  return (
    <span
      className="absolute top-1/2 flex items-center gap-2 -translate-y-1/2 whitespace-nowrap"
      style={{ left: x(event.at) }}
    >
      <span
        className={`-ml-[5px] size-2.5 shrink-0 border transition-colors duration-200 ${deploy ? 'rotate-45' : 'rounded-[2px]'} ${on ? 'bg-clover border-clover' : 'bg-[#09090a] border-white/45'}`}
        style={{
          boxShadow: latest
            ? '0 0 0 3px #09090a, 0 0 0 4px var(--color-clover-core)'
            : undefined,
        }}
      />
      <span
        className={`font-mono text-[11.5px] md:text-[12px] transition-colors duration-200 ${on ? 'text-white' : 'text-white/40'}`}
      >
        {event.label}
      </span>
    </span>
  );
}

export default function HeroTimeline() {
  const [minute, setMinute] = useState(0);
  const scroller = useRef<HTMLDivElement>(null);
  const touched = useRef(false);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setMinute(MINUTES);
      return;
    }
    const begin = performance.now();
    let last = -1;
    let id = 0;
    const tick = (now: number) => {
      const t = Math.max(0, now - begin) % (LAP_MS + HOLD_MS);
      const m = Math.min(1, t / LAP_MS) * MINUTES;
      const rounded = Math.round(m * 20) / 20;
      if (rounded !== last) {
        last = rounded;
        setMinute(rounded);
        // On a narrow screen the timeline scrolls inside its frame, and follows the playhead until the
        // reader scrolls it by hand.
        const el = scroller.current;
        if (el && !touched.current && el.scrollWidth > el.clientWidth) {
          const track = el.scrollWidth - el.clientWidth;
          el.scrollLeft = Math.max(
            0,
            Math.min(
              track,
              (m / MINUTES) * el.scrollWidth - el.clientWidth / 2,
            ),
          );
        }
      }
      id = requestAnimationFrame(tick);
    };
    id = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(id);
  }, []);

  const passed = EVENTS.filter((e) => e.at <= minute);
  const latest = passed.at(-1);
  const done = minute >= EXPERIMENT_END;

  return (
    <figure
      aria-label="A timeline of one function over half an hour, on five tracks: source changes, builds, deployments, traces, and experiments. At 14:02 the classify prompt is edited, build 318 passes at 14:03, v42 deploys at 14:05, and by 14:09 errors on classify fall from 4.1% to 0.6%. An experiment from 14:12 to 14:26 finds gpt-4o-mini matches gpt-4o's accuracy at 38% lower cost."
      className="relative pb-16"
    >
      <div className="rounded-md border border-white/12 bg-[#09090a]">
        <div className="flex flex-wrap items-center justify-between gap-x-8 gap-y-1 px-4 md:px-6 py-3 border-b border-white/12 font-mono text-[11.5px] md:text-[12px] text-white/50">
          <p className="flex gap-5">
            <span className="text-white/85">classify, one afternoon</span>
            <span>Fig. 3</span>
          </p>
          <p>
            error rate{' '}
            <span
              className="tabular-nums"
              style={{
                color:
                  errorRate(minute) > 1
                    ? '#FF9A9E'
                    : 'var(--color-clover-core)',
              }}
            >
              {errorRate(minute).toFixed(1)}%
            </span>
          </p>
        </div>

        <div className="flex">
          {/* Track names stay put while the tracks scroll on a narrow screen. */}
          <ul className="shrink-0 w-[104px] md:w-[150px] border-r border-white/10 text-[12px] md:text-[13px] text-white/60">
            <li aria-hidden className="h-8 border-b border-white/15" />
            {TRACKS.map((name, i) => (
              <li
                className={`flex items-center px-3 md:px-6 border-b border-white/[0.07] ${i === TRACE ? 'h-20' : 'h-12'}`}
                key={name}
              >
                {name}
              </li>
            ))}
          </ul>

          <div
            className="flex-1 min-w-0 overflow-x-auto"
            onPointerDown={() => {
              touched.current = true;
            }}
            ref={scroller}
          >
            <div className="relative min-w-[680px] mr-6 md:mr-10 ml-6 md:ml-8">
              {/* The time axis. */}
              <div className="relative h-8 border-b border-white/15">
                {Array.from({ length: MINUTES + 1 }, (_, m) => (
                  <span
                    className={`absolute bottom-0 w-px ${m % 5 ? 'h-1.5 bg-white/20' : 'h-2.5 bg-white/40'}`}
                    // biome-ignore lint/suspicious/noArrayIndexKey: one tick per minute
                    key={m}
                    style={{ left: x(m) }}
                  />
                ))}
                {Array.from({ length: MINUTES / 5 + 1 }, (_, i) => (
                  <span
                    className="absolute top-2 -translate-x-1/2 font-mono text-[11px] text-white/45"
                    // biome-ignore lint/suspicious/noArrayIndexKey: one label per five minutes
                    key={i}
                    style={{ left: x(i * 5) }}
                  >
                    {clock(i * 5)}
                  </span>
                ))}
              </div>

              {/* Each event's column, so the tracks read top to bottom. */}
              {EVENTS.map((e) => (
                <span
                  aria-hidden
                  className={`absolute top-8 bottom-0 w-px border-l border-dotted transition-colors duration-300 ${e === latest ? 'border-white/35' : 'border-white/10'}`}
                  key={`${e.track}-${e.at}`}
                  style={{ left: x(e.at) }}
                />
              ))}

              {TRACKS.map((name, i) => (
                <div
                  className={`relative border-b border-white/[0.07] ${i === TRACE ? 'h-20' : 'h-12'}`}
                  key={name}
                >
                  {i === DEPLOY && (
                    <>
                      {/* Which version is live. */}
                      <span
                        className="absolute top-1/2 h-px bg-white/25"
                        style={{
                          left: x(DEPLOYED),
                          right: `${100 - (21 / MINUTES) * 100}%`,
                        }}
                      />
                      <span
                        className="absolute top-1/2 h-px bg-white/25"
                        style={{ left: x(21), right: 0 }}
                      />
                    </>
                  )}
                  {i === TRACE && (
                    <div className="absolute inset-x-0 bottom-2 h-9 flex items-end gap-px">
                      {BARS.map((bar, b) => (
                        <span
                          className="flex-1 min-w-0 transition-opacity duration-200"
                          // biome-ignore lint/suspicious/noArrayIndexKey: one bar per 15 seconds
                          key={b}
                          style={{
                            background: bar.error
                              ? RED
                              : 'rgb(255 255 255 / 0.5)',
                            height: bar.error ? '100%' : `${bar.height}%`,
                            opacity: bar.at <= minute ? 1 : 0.22,
                          }}
                        />
                      ))}
                    </div>
                  )}
                  {i === EXPERIMENT && (
                    <span
                      className={`absolute top-1/2 h-6 -translate-y-1/2 flex items-center px-2.5 rounded-[2px] border font-mono text-[11.5px] md:text-[12px] whitespace-nowrap overflow-hidden transition-colors duration-300 ${minute >= EXPERIMENT_START ? 'border-clover/60 text-white' : 'border-white/15 text-white/40'}`}
                      style={{
                        background:
                          minute >= EXPERIMENT_START
                            ? 'rgb(from var(--color-clover-core) r g b / 0.12)'
                            : 'transparent',
                        left: x(EXPERIMENT_START),
                        width: `${((EXPERIMENT_END - EXPERIMENT_START) / MINUTES) * 100}%`,
                      }}
                    >
                      {
                        EVENTS.find(
                          (e) =>
                            e.track === EXPERIMENT &&
                            e.at === (done ? EXPERIMENT_END : EXPERIMENT_START),
                        )?.label
                      }
                    </span>
                  )}
                  {EVENTS.filter((e) => e.track === i && i !== EXPERIMENT).map(
                    (e) =>
                      i === TRACE ? (
                        <span
                          className={`absolute top-2 flex items-center gap-2 whitespace-nowrap font-mono text-[11.5px] md:text-[12px] transition-colors duration-200 ${e.at <= minute ? 'text-white' : 'text-white/40'}`}
                          key={e.label}
                          style={{ left: x(e.at) }}
                        >
                          <span
                            className={`-ml-[3px] size-1.5 rounded-full ${e.at <= minute ? 'bg-clover' : 'bg-white/40'}`}
                          />
                          {e.label}
                        </span>
                      ) : (
                        <Marker
                          event={e}
                          key={e.label}
                          latest={e === latest}
                          on={e.at <= minute}
                        />
                      ),
                  )}
                </div>
              ))}

              {/* The playhead. */}
              <span
                aria-hidden
                className="absolute top-0 bottom-0 w-px bg-clover"
                style={{ left: x(minute) }}
              >
                <span className="absolute -left-[3px] top-0 size-[7px] rotate-45 bg-clover" />
              </span>
            </div>
          </div>
        </div>

        <div className="grid md:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)] gap-x-12 gap-y-4 px-4 md:px-6 py-4 border-t border-white/12">
          <p className="text-[13.5px] md:text-[14px] leading-[1.55] text-white/55 max-w-[32rem]">
            <span className="text-white">Figure 3.</span> Every track shares one
            clock and one function name, so the toolchain can line up what
            changed with what happened in production.
          </p>
          <ol
            aria-live="polite"
            className="flex flex-col gap-1 font-mono text-[12px] md:text-[13px] leading-[1.5]"
          >
            {passed.slice(-4).map((e) => (
              <li
                className={`v3-enter flex gap-3 ${e === latest ? 'text-white' : 'text-white/40'}`}
                key={`${e.track}-${e.at}`}
              >
                <span className="shrink-0 text-white/35">{clock(e.at)}</span>
                {e.log}
              </li>
            ))}
          </ol>
        </div>
      </div>
    </figure>
  );
}
