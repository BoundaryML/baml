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

/**
 * Whether the learner held back when they should have.
 *
 * The comparison is how often they answered anyway when they did not know,
 * against how often the bar asks them to, and not how many answers were
 * right: selection holds the share right near whatever it aims at, so
 * accuracy would measure the sitting rather than the learner.
 */
function calibrated(standing: Standing, knobs: Knobs): string {
  const answers = standing.commits_unsure;
  if (answers === null) {
    return 'Nothing to judge that on yet.';
  }
  const asked = 1 - knobs.bar;
  if (answers > asked + 0.15) {
    return 'You answer more often than you should when you do not know. Below the bar, not sure is the better play.';
  }
  if (answers < asked - 0.15) {
    return 'You hold back more often than you need to. Above the bar, answering is the better play.';
  }
  return 'Well judged: you hold back about as often as the bar asks.';
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
        {standing.commits_unsure === null
          ? 'You have answered nothing, so there is nothing to judge.'
          : `When you did not know, you answered anyway ${percent(standing.commits_unsure)} of the time; the bar asks for ${percent(1 - knobs.bar)}. You held back on ${percent(standing.withheld)} of all the cases.`}
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
