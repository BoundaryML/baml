// The quiz itself, which lives in BAML.
//
// Everything this page knows about the type system, about which programs
// compile, about why, and about the learner, comes through these calls. The
// page picks what to show and when; it never decides an answer, never counts
// one, and never judges how a sitting is going.
//
// Only plain data goes in: strings, numbers, booleans, and the generated
// classes built from them. An enum argument arrives in BAML as a bare string,
// which compares unequal to every variant without erroring, so a surface that
// took one would mark every answer wrong and say nothing. A seed crosses as a
// bigint: a stream state uses 63 bits, and an `int` comes through the bridge
// as a JavaScript number, which keeps 53. Values coming back may be as rich
// as they like.

import {
  type Answered,
  adaptive_json,
  answer_case,
  describe_model,
  engine,
  fresh_profile,
  type Given,
  Given as GivenClass,
  type Next,
  next_prompt,
  replay as replayAnswers,
  type Taken,
  Taken as TakenClass,
} from '@quiz/sdk';

export { engine };
export type { Answered, Given, Next, Taken };
export type Claim = engine.Claim;
export type Exchange = engine.Exchange;
export type Knobs = engine.Knobs;
export type Points = engine.Points;
export type Profile = engine.Profile;
export type Prompt = engine.Prompt;
export type Reported = engine.Reported;
export type Revealed = engine.Revealed;
export type Rule = engine.Rule;
export type Verdicted = engine.Verdicted;
export type Standing = engine.Standing;
export const Why = engine.Why;

/**
 * What a learner said: of one program, whether the compiler accepts it; of
 * two side by side, which of them it accepts. Either way, that they would not
 * commit.
 */
export type Said = 'compiles' | 'rejected' | 'first' | 'second' | 'unsure';
const SAIDS: readonly string[] = [
  'compiles',
  'rejected',
  'first',
  'second',
  'unsure',
];

/** The words a question showing `programs` programs takes. */
export function wordsFor(programs: number): [Said, string][] {
  return programs > 1
    ? [
        ['first', 'The first compiles'],
        ['second', 'The second compiles'],
      ]
    : [
        ['compiles', 'It compiles'],
        ['rejected', 'It is rejected'],
      ];
}

export function isSaid(value: unknown): value is Said {
  return typeof value === 'string' && SAIDS.includes(value);
}

/**
 * What a learner said about one question, and who marked the reasoning:
 * the model's name when the grader did, empty when they marked their own.
 */
export function given(
  said: Said,
  reasoning: string,
  mark: string,
  markedBy = '',
): Given {
  return new GivenClass({ mark, marked_by: markedBy, reasoning, said });
}

/**
 * One question as the page keeps it: where its case came from, and what was
 * said about it.
 */
export function taken(item: string, seed: bigint, said: Given): Taken {
  return new TakenClass({ given: said, item, seed });
}

/** The knobs as data: what `Knobs` is built from, and what a save holds. */
export type KnobValues = ConstructorParameters<typeof engine.Knobs>[0];

export function defaultKnobs(): Knobs {
  return engine.default_knobs();
}

export function knobsFrom(values: KnobValues): Knobs {
  return new engine.Knobs(values);
}

/** The data of `knobs`, without the class around it. */
export function knobValues(knobs: Knobs): KnobValues {
  return { ...knobs };
}

/** What an answer is worth under `knobs`: right, wrong, and not sure. */
export function pointsOf(knobs: Knobs): Points {
  return engine.points(knobs);
}

export function freshProfile(): Profile {
  return fresh_profile();
}

/** The profile every answer in `history` leads to, from nothing. */
export function replay(knobs: Knobs, history: Taken[]): Profile {
  return replayAnswers(knobs, history);
}

/** Where a sitting stands: whether it is over and why, and what is known. */
export function standingOf(profile: Profile, knobs: Knobs): Standing {
  return engine.standing(profile, knobs);
}

/** The case the model would ask next, or `null` when the bank has nothing. */
export function nextPrompt(
  profile: Profile,
  knobs: Knobs,
  session: number,
  step: number,
): Next | null {
  return next_prompt(profile, knobs, session, step);
}

/**
 * Answer the case at `item` and `seed`. The case is worked out again from
 * those, so the answer never had to be on this page before it was given.
 */
export function answerCase(
  profile: Profile,
  knobs: Knobs,
  item: string,
  seed: bigint,
  said: Given,
): Answered {
  return answer_case(profile, knobs, item, seed, said);
}

/** The commit this build is of, stamped in at build time (vite.config.ts). */
declare const __TYPE_QUIZ_COMMIT__: string;

/**
 * The sitting as the JSON a learner takes away; `baml run review` reads it.
 *
 * Stamped with the commit the bridge, the SDK and this page were built from,
 * so a file that comes back weeks later says which bank generated its cases.
 */
export function transcriptJson(
  session: number,
  full: boolean,
  knobs: Knobs,
  history: Taken[],
): string {
  return adaptive_json(session, full, knobs, history, __TYPE_QUIZ_COMMIT__);
}

/** What a learner who reasons as the naive model `id` does believes. */
export function describeModel(id: string): string | null {
  return describe_model(id);
}
