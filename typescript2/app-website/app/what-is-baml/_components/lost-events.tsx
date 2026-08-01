'use client';

import { useSyncExternalStore } from 'react';
import {
  getBaml,
  getBamlServer,
  getDropped,
  getStartServer,
  setBaml,
  subscribeDropped,
} from './dropped-store';

// Dramatizes the section's claim. Under OTEL you sample, so most events fall
// past unreadable and stay that way. Under BAML every event is traced. The
// counter only moves in OTEL mode, and it never goes back down: you cannot
// retroactively trace what already ran.

// Each event belongs to a part of the system. Traced, you get the area and
// what happened. Dropped, you get a guess at the area and nothing else.
const KINDS = [
  { area: 'auth', detail: 'token expired' },
  { area: 'payments', detail: 'timeout 4.2s' },
  { area: 'checkout', detail: '429 rate_limit' },
  { area: 'search', detail: 'tool call 812ms' },
  { area: 'webhooks', detail: 'retry 2/3' },
  { area: 'ingest', detail: 'parse failed' },
  { area: 'billing', detail: '$0.0031' },
  { area: 'uploads', detail: 'cache miss' },
  { area: 'sessions', detail: '1,204 tok' },
  { area: 'inference', detail: 'fallback → haiku' },
];

// Deterministic layout (no Math.random) so server and client markup match.
// Every 10th event survives OTEL's sampler.
const EVENTS = Array.from({ length: 34 }, (_, i) => ({
  id: i,
  left: (i * 29) % 68,
  delay: (i * 0.53) % 11,
  ...KINDS[(i * 3) % KINDS.length],
  // OTEL loses events two ways: the sampler throws some away, and the rest
  // were never instrumented in the first place.
  sampled: i % 10 === 0,
  ghost: i % 3 === 0,
}));

export function LostEvents() {
  const baml = useSyncExternalStore(subscribeDropped, getBaml, getBamlServer);
  const dropped = useSyncExternalStore(
    subscribeDropped,
    getDropped,
    getStartServer,
  );

  return (
    <div className="lev">
      <div aria-hidden="true" className="lev-stage">
        {EVENTS.map((e) => {
          const traced = baml || e.sampled;
          const lost = !traced && e.ghost ? 'never captured' : 'sampled out';
          return (
            <span
              className={`lev-dot${traced ? ' lev-dot--on' : ''}${
                !traced && e.ghost ? ' lev-dot--ghost' : ''
              }`}
              key={e.id}
              style={{ animationDelay: `${e.delay}s`, left: `${e.left}%` }}
            >
              <span className="lev-face">
                {traced ? `${e.area} · ${e.detail}` : '?'}
              </span>
              {/* a dropped event can only tell you roughly where it came from */}
              <span className="lev-alt">
                {traced ? `${e.area} · ${e.detail}` : `${lost} · ${e.area}?`}
              </span>
            </span>
          );
        })}
      </div>

      <div className="lev-bar">
        <div className="lev-seg" role="group">
          <button
            aria-pressed={!baml}
            className={`lev-segbtn lev-segbtn--otel${
              baml ? '' : ' lev-segbtn--on'
            }`}
            onClick={() => setBaml(false)}
            type="button"
          >
            OTEL
          </button>
          <button
            aria-pressed={baml}
            className={`lev-segbtn lev-segbtn--baml${
              baml ? ' lev-segbtn--on' : ''
            }`}
            onClick={() => setBaml(true)}
            type="button"
          >
            BAML
          </button>
        </div>
        <p className="lev-note">
          {baml ? (
            <>Everything traced. The {dropped} OTEL dropped are still gone.</>
          ) : (
            <>
              {dropped} events dropped. You could add a log line and wait for it
              to happen again.
            </>
          )}
        </p>
      </div>
    </div>
  );
}
