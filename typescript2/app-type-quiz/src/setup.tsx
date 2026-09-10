// Choosing a sitting: until mastered by default, practice by count, and the
// knobs behind both.

import { useState } from 'react';
import { PointsRule, percent } from './panels';
import {
  defaultKnobs,
  type KnobValues,
  knobsFrom,
  knobValues,
  pointsOf,
} from './quiz';

export interface Chosen {
  full: boolean;
  knobs: KnobValues;
}

/** Abstain bars a learner may pick, as the confidence at which to commit. */
const BARS = [0.5, 0.67, 0.75, 0.8];
/** Certainties a rule may be required to reach. */
const TARGETS = [0.75, 0.85, 0.95];

export function Setup({
  ready,
  onStart,
  onBack,
}: {
  ready: boolean;
  onStart: (chosen: Chosen) => void;
  onBack: () => void;
}) {
  const [defaults] = useState(() => knobValues(defaultKnobs()));
  const [practice, setPractice] = useState(false);
  const [count, setCount] = useState(10);
  const [full, setFull] = useState(false);
  const [bar, setBar] = useState(defaults.bar);
  const [strict, setStrict] = useState(defaults.strict);
  const [target, setTarget] = useState(defaults.target);
  const [budget, setBudget] = useState(defaults.budget);
  const knobs: KnobValues = {
    ...defaults,
    bar,
    budget: practice ? count : budget,
    practice,
    strict,
    target,
  };
  return (
    <section className="setup">
      <fieldset className="kinds">
        <label>
          <input
            checked={!practice}
            name="kind"
            onChange={() => setPractice(false)}
            type="radio"
          />
          Until mastered: as many cases as it takes to be confident in your
          intuition for the type system
        </label>
        <label>
          <input
            checked={practice}
            name="kind"
            onChange={() => setPractice(true)}
            type="radio"
          />
          Practice:
          <input
            className="count"
            max={200}
            min={1}
            onChange={(e) => setCount(Number(e.target.value))}
            type="number"
            value={count}
          />
          cases, adapting all the way
        </label>
      </fieldset>
      <label className="check">
        <input
          checked={full}
          onChange={(e) => setFull(e.target.checked)}
          type="checkbox"
        />
        Ask why, and mark your own reasoning after each case
      </label>
      <details>
        <summary>Tuning</summary>
        <label>
          Commit when you are at least this sure
          <select
            onChange={(e) => setBar(Number(e.target.value))}
            value={String(bar)}
          >
            {BARS.map((b) => (
              <option key={b} value={String(b)}>
                {percent(b)}
              </option>
            ))}
          </select>
        </label>
        <PointsRule points={pointsOf(knobsFrom(knobs))} />
        <label className="check">
          <input
            checked={strict}
            onChange={(e) => setStrict(e.target.checked)}
            type="checkbox"
          />
          Insist on every rule separately, not just the overall picture
        </label>
        <label>
          Certainty a rule needs before it counts as mastered
          <select
            onChange={(e) => setTarget(Number(e.target.value))}
            value={String(target)}
          >
            {TARGETS.map((t) => (
              <option key={t} value={String(t)}>
                {percent(t)}
              </option>
            ))}
          </select>
        </label>
        {!practice && (
          <label>
            Most cases to ask
            <input
              max={400}
              min={1}
              onChange={(e) => setBudget(Number(e.target.value))}
              type="number"
              value={budget}
            />
          </label>
        )}
      </details>
      <div className="choices">
        <button
          disabled={!ready}
          onClick={() => onStart({ full, knobs })}
          type="button"
        >
          {ready ? 'Start' : 'Loading…'}
        </button>
        <button className="quiet" onClick={onBack} type="button">
          Back
        </button>
      </div>
      <p className="note">
        No running score is shown. The cases follow you, so how many you get
        right measures the sitting rather than you; each answer's points are
        shown as you go, and mastery and calibration are read out at the end.
      </p>
    </section>
  );
}
