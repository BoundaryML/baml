// The quiz itself, which lives in BAML.
//
// Everything this page knows about the type system, about which programs
// compile, and about why, comes through these calls. The page picks what to
// show and when; it never decides an answer, and it never counts one.
//
// Only plain data goes in: a bool and two strings per answer. An enum argument
// arrives in BAML as a bare string, which compares unequal to every variant
// without erroring, so a surface that took one would mark every answer wrong
// and say nothing. Values coming back may be as rich as they like.

import {
  answer_at,
  compiler_report,
  engine,
  type Given,
  Given as GivenClass,
  prompt_at,
  sitting_json,
  sitting_length,
  sitting_score,
} from '@quiz/sdk';

export { engine };
export type Prompt = engine.Prompt;
export type Exchange = engine.Exchange;
export type Reported = engine.Reported;
export type Score = engine.Score;
export type { Given };

/** How a sitting is identified: the same pair asks the same questions. */
export interface Sitting {
  seed: number;
  length: number;
}

/** What a learner said about one question. */
export function given(
  compiles: boolean,
  reasoning: string,
  mark: string,
): Given {
  return new GivenClass({ compiles, mark, reasoning });
}

export function sittingLength({ seed, length }: Sitting): number {
  return sitting_length(seed, length);
}

/** The program at `index`, carrying no answer, or `null` past the end. */
export function promptAt(
  { seed, length }: Sitting,
  index: number,
): Prompt | null {
  return prompt_at(seed, length, index);
}

/**
 * Answer the question at `index`. The case is worked out again from the seed
 * and the verdict marked against its key, so the answer never had to be on
 * this page before it was given.
 */
export function answerAt(
  { seed, length }: Sitting,
  index: number,
  said: Given,
): Exchange | null {
  return answer_at(seed, length, index, said);
}

/** What the compiler itself says about the case at `index`. */
export function compilerReport(
  { seed, length }: Sitting,
  index: number,
): Reported | null {
  return compiler_report(seed, length, index);
}

/** How the sitting has gone, counted by the engine rather than here. */
export function score(
  { seed, length }: Sitting,
  full: boolean,
  answers: Given[],
): Score {
  return sitting_score(seed, length, full, answers);
}

/** The sitting as the JSON a learner takes away; `baml run review` reads it. */
export function transcriptJson(
  { seed, length }: Sitting,
  full: boolean,
  answers: Given[],
): string {
  return sitting_json(seed, length, full, answers);
}
