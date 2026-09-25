// The grader, from the page's side: the learner's keys and chosen model,
// kept in this browser and nowhere else, and one call per judgement.
//
// A key goes into exactly one request, to the provider that issued it. The
// site is static and has no server of its own, so there is nothing of ours
// for a key to reach; storing them here rather than asking every time is a
// convenience the learner can undo by clearing the field.
//
// Keys are held one per provider, because that is what they belong to: a
// learner with an Anthropic key and an OpenAI key can move between models
// without pasting either again, and a key can never be sent to the provider
// that did not issue it.
//
// They live in the tab rather than in the browser. Storage is scoped to an
// origin and not to a path, and this site shares its origin with every other
// page the organisation publishes there, so a key left in local storage is
// readable by all of them for as long as it sits there.

import { engine } from '@quiz/sdk';
import type { Revealed } from './quiz';
import { keyStoreOf } from './saves';

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

export type Offer = engine.Offer;

export interface JudgeSettings {
  /** A key per provider, by the provider's own name. */
  keys: Record<string, string>;
  model: string;
}

/** The models a judgement may be asked of, best first; the engine's table. */
export function models(): Offer[] {
  return engine.offered();
}

/** The offer the settings name. */
export function offerOf(model: string): Offer {
  return engine.offer_of(model);
}

/**
 * How the provider behind a model is put to a learner.
 *
 * Off the row rather than asked for: an enum argument does not survive the
 * generated web SDK, so nothing here hands a `Provider` back to the engine.
 */
export function wordsFor(model: string): engine.ProviderWords {
  return offerOf(model).words;
}

/** The key for a model's provider, or empty when there is none for it. */
export function keyFor(settings: JudgeSettings, model: string): string {
  return settings.keys[offerOf(model).provider] ?? '';
}

/** The settings with `key` set for the provider of the chosen model. */
export function withKey(settings: JudgeSettings, key: string): JudgeSettings {
  return {
    ...settings,
    keys: { ...settings.keys, [offerOf(settings.model).provider]: key },
  };
}

function fresh(): JudgeSettings {
  return { keys: {}, model: models()[0].id };
}

export function loadJudge(): JudgeSettings {
  try {
    const raw = keyStoreOf().getItem(KEY);
    if (raw === null) {
      return fresh();
    }
    const value: unknown = JSON.parse(raw);
    if (typeof value !== 'object' || value === null) {
      return fresh();
    }
    const { key, keys, model } = value as {
      key?: unknown;
      keys?: unknown;
      model?: unknown;
    };
    const offers = models();
    const chosen =
      typeof model === 'string' && offers.some((o) => o.id === model)
        ? model
        : offers[0].id;
    const held: Record<string, string> = {};
    if (typeof keys === 'object' && keys !== null) {
      for (const [provider, value] of Object.entries(keys)) {
        if (typeof value === 'string') {
          held[provider] = value;
        }
      }
    }
    // Settings written before there was more than one provider held a single
    // key, which belonged to whichever model was chosen then.
    if (typeof key === 'string' && key !== '') {
      held[offerOf(chosen).provider] ??= key;
    }
    return { keys: held, model: chosen };
  } catch {
    return fresh();
  }
}

/**
 * Keep the settings for next time, or do not: this runs on every keystroke in
 * the key field, and a browser with storage blocked or full would otherwise
 * throw once per character into an event handler that has nowhere to put it.
 * The settings are already held in the panel's own state, so a visit that
 * cannot save still judges.
 */
export function saveJudge(settings: JudgeSettings): void {
  try {
    keyStoreOf().setItem(KEY, JSON.stringify(settings));
  } catch {
    return;
  }
}

/** Whether a judgement can be asked for at all. */
export function judgeReady(settings: JudgeSettings): boolean {
  return keyFor(settings, settings.model).trim() !== '';
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
    keyFor(settings, settings.model).trim(),
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
 * drifting apart. It takes the grade, not its understanding, because an enum
 * argument does not survive this boundary.
 */
function markOf(grade: Grade): string {
  return engine.mark_of(grade);
}
