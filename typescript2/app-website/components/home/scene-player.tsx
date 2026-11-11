'use client';

import { Pause, Play, RotateCcw } from 'lucide-react';
import { type ComponentType, useEffect, useRef, useState } from 'react';
import { useReducedMotion, useVisible } from '@/components/home/pinned';

// A set of short animated scenes, as tabs, with the controls readers asked for on the pricing page:
// each scene plays a step at a time while it's on screen, holds when done, then moves to the next
// (a bar fills on the way) until the reader takes over. A click on the scene pauses or plays it;
// the bar underneath has play/pause/replay, a dot per step to jump to, and the next scene's name
// once the scene is done. Scenes draw themselves from `step`, so each one sets its own look.

export type SceneDef = {
  /** How many steps the scene plays. */
  steps: number;
  title: string;
  View: ComponentType<{ step: number }>;
};

const STEP_MS = 1300;
// The pause before the next scene: one more bar, at half the steps' pace.
const HOLD_MS = STEP_MS * 2;

/** The scenes as tabs: they play in turn while on screen, and a click jumps to one. */
export function ScenePlayer({ scenes: SCENES }: { scenes: SceneDef[] }) {
  const ref = useRef<HTMLDivElement>(null);
  const visible = useVisible(ref);
  const reduce = useReducedMotion();
  const [scene, setScene] = useState(0);
  const [step, setStep] = useState(0);
  // Plays a scene once, then stops on it so it can be read; a click pauses or plays it.
  const [playing, setPlaying] = useState(true);
  // Until the reader takes over (a pause, a step, a tab), a finished scene holds, then moves on.
  const [auto, setAuto] = useState(true);
  const current = SCENES[scene] ?? SCENES[0];
  const last = current.steps;
  const done = step >= last;

  useEffect(() => {
    if (reduce) {
      setStep(last);
      return;
    }
    if (!visible || !playing) return;
    if (done) {
      if (!auto) return;
      const hold = setTimeout(() => {
        setScene((n) => (n + 1) % SCENES.length);
        setStep(0);
      }, HOLD_MS);
      return () => clearTimeout(hold);
    }
    const timer = setTimeout(
      () => setStep((n) => n + 1),
      step === 0 ? 600 : STEP_MS,
    );
    return () => clearTimeout(timer);
  }, [step, last, visible, reduce, playing, done, auto, SCENES.length]);

  const go = (i: number) => {
    setAuto(false);
    setScene(i);
    setStep(reduce ? SCENES[i].steps : 0);
    setPlaying(true);
  };
  // After the last scene, next is the first.
  const nextIndex = (scene + 1) % SCENES.length;
  const next = SCENES[nextIndex];

  const View = current.View;
  return (
    <div
      className="overflow-hidden rounded-lg border border-rule bg-raised"
      ref={ref}
    >
      <div className="flex gap-1 border-b border-rule px-2 pt-2">
        {SCENES.map((sc, i) => (
          <button
            className={`relative cursor-pointer whitespace-nowrap px-3 pt-1.5 pb-2.5 text-[13px] transition-colors ${i === scene ? 'text-head' : 'text-muted hover:text-ink'}`}
            key={sc.title}
            onClick={() => go(i)}
            type="button"
          >
            {sc.title}
            <span
              className="absolute inset-x-3 bottom-0 h-0.5 rounded-full bg-clover transition-opacity"
              style={{ opacity: i === scene ? 1 : 0 }}
            />
          </button>
        ))}
      </div>
      {/* A click anywhere on the scene pauses it, or plays it again. */}
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: the controls below do the same from the keyboard */}
      <div
        className="cursor-pointer"
        onClick={() => {
          setAuto(false);
          if (done) setStep(0);
          setPlaying((p) => (done ? true : !p));
        }}
      >
        <View step={step} />
      </div>
      <div className="flex items-center gap-3 border-t border-rule px-4 py-2.5 text-[12.5px]">
        <button
          aria-label={done ? 'Replay' : playing ? 'Pause' : 'Play'}
          className="focus-ring grid size-7 cursor-pointer place-items-center rounded-full bg-white/[0.06] text-ink hover:bg-white/10"
          onClick={() => {
            setAuto(false);
            if (done) setStep(0);
            setPlaying((p) => (done ? true : !p));
          }}
          type="button"
        >
          {done ? (
            <RotateCcw className="size-3.5" />
          ) : playing ? (
            <Pause className="size-3.5" />
          ) : (
            <Play className="size-3.5" />
          )}
        </button>
        {/* One dot a step: click one to jump there and stop. */}
        <div className="flex flex-1 items-center gap-1.5">
          {Array.from({ length: last }, (_, i) => (
            <button
              aria-label={`Step ${i + 1}`}
              className="h-1.5 flex-1 cursor-pointer rounded-full transition-colors"
              // biome-ignore lint/suspicious/noArrayIndexKey: steps
              key={i}
              onClick={() => {
                setAuto(false);
                setStep(i + 1);
                setPlaying(false);
              }}
              style={{
                background:
                  i < step ? 'var(--color-clover)' : 'rgba(255,255,255,0.1)',
              }}
              type="button"
            />
          ))}
          {/* One more bar, twice a step's length, that fills on the way to the next scene. */}
          {auto && (
            <span className="relative h-1.5 flex-[2] overflow-hidden rounded-full bg-white/10">
              {done && playing && visible && (
                <span
                  className="meter-next absolute inset-y-0 left-0 rounded-full bg-white/40"
                  key={scene}
                  style={{ animationDuration: `${HOLD_MS}ms` }}
                />
              )}
              <style>
                {
                  '.meter-next{width:0;animation-name:meter-next;animation-timing-function:linear;animation-fill-mode:forwards}@keyframes meter-next{to{width:100%}}'
                }
              </style>
            </span>
          )}
        </div>
        {/* Shown only once the scene is done, with the last bar; it keeps its room so nothing moves. */}
        <button
          aria-hidden={!done}
          className={`cursor-pointer whitespace-nowrap text-ink-2 transition-[opacity,color] duration-300 hover:text-head ${done ? '' : 'invisible opacity-0'}`}
          onClick={() => go(nextIndex)}
          tabIndex={done ? 0 : -1}
          type="button"
        >
          Next: {next.title} →
        </button>
      </div>
    </div>
  );
}
