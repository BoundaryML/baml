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

  useEffect(() => {
    // Fade the current name out, swap it, fade the next one in.
    const hold = setTimeout(() => {
      setOut(true);
      const swap = setTimeout(() => {
        setI((n) => (n + 1) % ACQUIRERS.length);
        setOut(false);
      }, FADE_MS);
      return () => clearTimeout(swap);
    }, HOLD_MS);
    return () => clearTimeout(hold);
  }, [i]);

  return (
    <span className="acq">
      {/* The widest name reserves the space so the headline never reflows. */}
      <span aria-hidden="true" className="acq-ghost">
        Microsoft
      </span>
      <span className={`acq-name${out ? ' acq-name--out' : ''}`}>
        {ACQUIRERS[i]}
      </span>
    </span>
  );
}
