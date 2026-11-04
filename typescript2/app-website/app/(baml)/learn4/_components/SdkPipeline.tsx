'use client';

import { useAnimateInView } from '../../learn3/_lib/use-animate-in-view';

/**
 * The generation pipeline as four hand-drawn steps. Plain text arrows,
 * staggered fade-in on mount — deliberately quiet.
 */
const STEPS = [
  { name: 'baml_src/', what: 'functions, types, tests' },
  { name: 'baml generate', what: 'one command' },
  { name: 'baml_sdk/', what: 'typed client + runtime inside' },
  { name: 'your app', what: 'from baml_sdk import b' },
];

export function SdkPipeline() {
  const { ref, holdClass } = useAnimateInView();
  return (
    <div className={`l4-pipe${holdClass}`} ref={ref}>
      {STEPS.map((s, i) => (
        <div key={s.name} className="l4-pipe-item">
          {i > 0 ? (
            <span
              className="l4-pipe-arrow"
              style={{ animationDelay: `${0.25 + i * 0.35}s` }}
              aria-hidden
            >
              →
            </span>
          ) : null}
          <div
            className={`l4-pipe-step ${i % 2 ? 'l4-sketch-b' : 'l4-sketch-a'}`}
            style={{ animationDelay: `${0.1 + i * 0.35}s` }}
          >
            <span className="l4-pipe-name font-mono">{s.name}</span>
            <span className="l4-pipe-what">{s.what}</span>
          </div>
        </div>
      ))}
    </div>
  );
}
