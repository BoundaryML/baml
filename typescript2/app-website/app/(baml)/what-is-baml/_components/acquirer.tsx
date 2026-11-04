'use client';

import { useEffect, useState } from 'react';

// The section title asks whether we are trying to get bought, and the name of
// the buyer keeps changing. Cycling it is the joke: it does not matter which
// one, the answer is the same.
const ACQUIRERS = ['Anthropic', 'OpenAI', 'Google', 'Microsoft', 'Amazon'];

const HOLD_MS = 1800;
const FADE_MS = 260;

export function Acquirer() {
  const [i, setI] = useState(0);
  const [out, setOut] = useState(false);

  // biome-ignore lint/correctness/useExhaustiveDependencies: re-arm per name; FADE_MS/HOLD_MS are module constants
  useEffect(() => {
    // Fade the current name out, swap it, fade the next one in. The inner
    // timeout must be cleared by the effect cleanup, not the setTimeout
    // callback's return value (which setTimeout discards).
    let swap: ReturnType<typeof setTimeout> | undefined;
    const hold = setTimeout(() => {
      setOut(true);
      swap = setTimeout(() => {
        setI((n) => (n + 1) % ACQUIRERS.length);
        setOut(false);
      }, FADE_MS);
    }, HOLD_MS);
    return () => {
      clearTimeout(hold);
      if (swap) clearTimeout(swap);
    };
  }, [i]);

  return (
    <span className="acq">
      {/* The widest name reserves the space so the headline never reflows. The
          question mark rides inside, so the slack falls after it rather than
          opening a gap between the name and the punctuation. */}
      <span aria-hidden="true" className="acq-ghost">
        Microsoft?
      </span>
      <span className={`acq-name${out ? ' acq-name--out' : ''}`}>
        {ACQUIRERS[i]}?
      </span>
    </span>
  );
}
