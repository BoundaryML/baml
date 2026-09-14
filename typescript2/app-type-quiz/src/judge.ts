// The grader, from the page's side: the learner's key and model, kept in
// this browser and nowhere else, and one call per judgement.
//
// The key goes into exactly one request, to Anthropic, with the header that
// provider documents for a call made from a browser. The site is static and
// has no server of its own, so there is nothing of ours for the key to reach;
// storing it here rather than asking every time is a convenience the learner
// can undo by clearing it.

import { engine } from '@quiz/sdk';
import type { Revealed } from './quiz';

export type Grade = engine.Grade;

/**
 * What came of asking: a grade with the mark it amounts to, or why there is
 * none. The engine hands its failures back as values rather than throwing
 * them, and the two outcomes are told apart here, so nothing above this file
 * has to know which of the union it is holding.
 */
export type Judgement =
  | { kind: 'grade'; grade: Grade; mark: string }
  | { kind: 'failed'; why: string };

const KEY = 'type-quiz/judge';

export interface JudgeSettings {
  key: string;
  model: string;
}

/** The models offered, first is the default. */
export const MODELS: ReadonlyArray<[string, string]> = [
  ['claude-sonnet-4-5', 'Claude Sonnet 4.5'],
  ['claude-haiku-4-5', 'Claude Haiku 4.5'],
];

export function loadJudge(): JudgeSettings {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (raw === null) {
      return { key: '', model: MODELS[0][0] };
    }
    const value: unknown = JSON.parse(raw);
    if (typeof value !== 'object' || value === null) {
      return { key: '', model: MODELS[0][0] };
    }
    const { key, model } = value as { key?: unknown; model?: unknown };
    return {
      key: typeof key === 'string' ? key : '',
      model:
        typeof model === 'string' && MODELS.some(([id]) => id === model)
          ? model
          : MODELS[0][0],
    };
  } catch {
    return { key: '', model: MODELS[0][0] };
  }
}

export function saveJudge(settings: JudgeSettings): void {
  window.localStorage.setItem(KEY, JSON.stringify(settings));
}

/** Whether a judgement can be asked for at all. */
export function judgeReady(settings: JudgeSettings): boolean {
  return settings.key.trim() !== '';
}

/**
 * Mark the reasoning behind an answer against the case's own explanation.
 *
 * The revealed case goes over whole: which programs were shown, what the
 * compiler made of each and which claims the answer's question turns on are
 * all worked out in the engine, so this cannot hand the grader a rubric or a
 * verdict that belongs to some other question.
 */
export async function judgeReasoning(
  settings: JudgeSettings,
  revealed: Revealed,
  said: string,
  reasoning: string,
): Promise<Judgement> {
  const judged = await engine.judge_reasoning_async(
    settings.key.trim(),
    settings.model,
    revealed,
    said,
    reasoning,
  );
  return judged instanceof engine.Unjudged
    ? { kind: 'failed', why: judged.why }
    : { grade: judged, kind: 'grade', mark: markOf(judged) };
}

/**
 * The mark a grade amounts to, in the words a mark is kept in.
 *
 * The engine's own word for the understanding, rather than a word of the
 * page's: the same three words name a mark the learner made by hand, and one
 * account of them is what keeps a judged mark and a self-made one from
 * drifting apart.
 */
function markOf(grade: Grade): string {
  return engine.understanding_word(grade.understanding);
}
