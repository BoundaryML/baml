// What a sitting concluded: mastery and calibration, and never a total.
//
// Two readings, kept apart. Mastery is the model's estimate, adjusted for
// difficulty and independent of how long the sitting ran. Calibration is
// whether the learner committed when they should have: committed accuracy
// against the bar they were asked to commit at, and how often they held back.

import { Prose, percent } from './panels';
import {
  describeModel,
  type Knobs,
  type Profile,
  type Standing,
  Why,
} from './quiz';

function ended(standing: Standing, knobs: Knobs, starved: boolean): string {
  if (starved) {
    return 'The bank has nothing left that suits you.';
  }
  switch (standing.why) {
    case Why.Mastered:
      return 'The model is confident in your intuition for the type system.';
    case Why.Budget:
      return knobs.practice
        ? `That was the ${knobs.budget} cases you asked for.`
        : `The budget of ${knobs.budget} cases is spent.`;
    case Why.Stalled:
      return `No estimate has moved in the last ${knobs.stall_after} cases, so the sitting stopped rather than grind.`;
    case Why.Continue:
      return 'The sitting was left before it was over.';
  }
}

function calibrated(standing: Standing, knobs: Knobs): string {
  const accuracy = standing.calibration;
  if (accuracy === null) {
    return 'You committed to nothing, so there is nothing to compare with the bar. Commit when you are surer than it; not sure is for the rest.';
  }
  const gap = accuracy - knobs.bar;
  if (gap > 0.1) {
    return 'You were right more often than the bar asks, so you could commit more often.';
  }
  if (gap < -0.1) {
    return 'You committed more often than you should have: below the bar, not sure is the better play.';
  }
  return 'Well calibrated: you committed about as often as you should.';
}

export function Readout({
  profile,
  knobs,
  standing,
  starved,
}: {
  profile: Profile;
  knobs: Knobs;
  standing: Standing;
  starved: boolean;
}) {
  const suspected =
    standing.suspected === null ? null : describeModel(standing.suspected);
  return (
    <div className="readout">
      <p className="why">{ended(standing, knobs, starved)}</p>
      <h3>Mastery</h3>
      <p>
        {standing.mastered.length} of {profile.skills.length} rules mastered.
      </p>
      {standing.shaky.length > 0 && (
        <>
          <p>Least certain, worst first:</p>
          <ul>
            {standing.shaky.slice(0, 6).map((rule) => (
              <li key={rule}>
                <code>{rule}</code>
              </li>
            ))}
          </ul>
        </>
      )}
      <h3>Calibration</h3>
      <p>
        {standing.calibration === null
          ? 'You held back on every case.'
          : `Of the answers you committed to, ${percent(standing.calibration)} were right, against a bar of ${percent(knobs.bar)}. You held back on ${percent(standing.withheld)}.`}
      </p>
      <p className="note">{calibrated(standing, knobs)}</p>
      {suspected !== null && (
        <>
          <h3>How you seem to reason</h3>
          <p>
            <Prose text={suspected} />
          </p>
        </>
      )}
    </div>
  );
}
