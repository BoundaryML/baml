// Choosing a sitting: until mastered by default, practice by count, and the
// knobs behind both.

import { useState } from 'react';
import {
  type JudgeSettings,
  judgeReady,
  keyFor,
  loadJudge,
  models,
  saveJudge,
  withKey,
  wordsFor,
} from './judge';
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

/**
 * How often a case that can be shown beside its opposite is. Only a case
 * that turns on a strict relation has an opposite, which is about a third of
 * them, so the share of a whole sitting shown this way is about half of
 * whatever is chosen here.
 */
const CONTRASTS: [number, string][] = [
  [0, 'Never'],
  [0.5, 'Half the time'],
  [1, 'Whenever the case allows'],
];

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
  // A number field hands back whatever was typed, including nothing and
  // fractions of a question. The bounds on the input are a hint to a mouse
  // and are not enforced against a keyboard, and what gets through is asked
  // of an engine whose budget is a whole number of cases: a budget of zero
  // ends the sitting before it starts, and 1.5 reaches the page as
  // "Case 1 of 1.5" and is saved that way.
  const whole = (
    text: string,
    low: number,
    high: number,
    fallback: number,
  ): number => {
    const asked = Math.trunc(Number(text));
    return Number.isFinite(asked)
      ? Math.min(Math.max(asked, low), high)
      : fallback;
  };
  const [practice, setPractice] = useState(false);
  const [count, setCount] = useState(10);
  const [bar, setBar] = useState(defaults.bar);
  const [strict, setStrict] = useState(defaults.strict);
  const [target, setTarget] = useState(defaults.target);
  const [budget, setBudget] = useState(defaults.budget);
  const [contrast, setContrast] = useState(defaults.contrast);
  const [judge, setJudge] = useState<JudgeSettings>(loadJudge);
  const keepJudge = (next: JudgeSettings) => {
    setJudge(next);
    saveJudge(next);
  };
  // Giving reasons is what a judge reads, so a sitting with one to ask asks
  // for them: until the learner says otherwise, this follows the key rather
  // than being sampled from it once. Setting a key and finding that nothing
  // is judged — because the box happened to be drawn before the key was
  // pasted — is the whole failure this is here to prevent. Without a key the
  // marking would fall to the learner, which is a thing to opt into rather
  // than to be handed.
  const [asking, setAsking] = useState<boolean | null>(null);
  const full = asking ?? judgeReady(judge);
  const chosen = wordsFor(judge.model);
  const knobs: KnobValues = {
    ...defaults,
    bar,
    budget: practice ? count : budget,
    contrast,
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
            onChange={(e) => setCount(whole(e.target.value, 1, 200, 10))}
            type="number"
            value={count}
          />
          cases, adapting all the way
        </label>
      </fieldset>
      <label className="check">
        <input
          checked={full}
          onChange={(e) => setAsking(e.target.checked)}
          type="checkbox"
        />
        {judgeReady(judge)
          ? 'Ask why a program is rejected, and have the judge mark your reasoning'
          : 'Ask why a program is rejected, and mark your own reasoning'}
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
        <label>
          Show two programs at once and ask which one compiles
          <select
            onChange={(e) => setContrast(Number(e.target.value))}
            value={String(contrast)}
          >
            {CONTRASTS.map(([rate, label]) => (
              <option key={rate} value={String(rate)}>
                {label}
              </option>
            ))}
          </select>
        </label>
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
              onChange={(e) =>
                setBudget(whole(e.target.value, 1, 400, defaults.budget))
              }
              type="number"
              value={budget}
            />
          </label>
        )}
      </details>
      <details>
        <summary>Judge</summary>
        <p className="note">
          With a key, a model marks your reasoning against the case's own
          explanation, in place of your marking it yourself, and says what the
          reasoning did not reach. That is one call on your key per case you
          give a reason for, and you are not asked to mark your own while there
          is a key set. A key is kept in this browser and sent only to the
          provider that issued it, with each judgement; this site has no server,
          and nothing of ours ever sees it. Keys are held one per provider, so
          moving between models does not ask for either again. Clear the field
          to stop.
        </p>
        <label>
          Model
          <select
            onChange={(e) => keepJudge({ ...judge, model: e.target.value })}
            value={judge.model}
          >
            {models().map((offer) => (
              <option key={offer.id} value={offer.id}>
                {offer.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          {chosen.name} API key
          <input
            autoComplete="off"
            onChange={(e) => keepJudge(withKey(judge, e.target.value))}
            placeholder={chosen.key_hint}
            spellCheck={false}
            type="password"
            value={keyFor(judge, judge.model)}
          />
        </label>
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
