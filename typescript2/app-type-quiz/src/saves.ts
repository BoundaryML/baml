// Save slots, like a game's.
//
// A slot holds what the page keeps of a sitting and nothing the sitting can
// work out for itself: the session, the knobs, and what was said about each
// case. The profile is rebuilt by replaying those on load, so a save can
// never hold a profile its answers do not lead to; and a save made against a
// quiz whose bank or model has since changed fails to replay and says so,
// instead of quietly resuming with beliefs its answers no longer support.

import { defaultKnobs, isSaid, type KnobValues, type Said } from './quiz';

export const SLOTS = 3;
export const VERSION = 1;

/** One question, as a save holds it. */
export interface SavedAnswer {
  item: string;
  /** The stream state in decimal: JSON has no bigint. */
  seed: string;
  said: Said;
  reasoning: string;
  mark: string;
}

export interface Save {
  version: typeof VERSION;
  session: number;
  full: boolean;
  knobs: KnobValues;
  answers: SavedAnswer[];
  /** When the slot was last written, as ISO 8601. */
  updated: string;
}

export type Slot =
  | { kind: 'empty' }
  | { kind: 'held'; save: Save }
  | { kind: 'unreadable'; reason: string };

function keyOf(slot: number): string {
  return `type-quiz/slot/${slot}`;
}

export function readSlot(storage: Storage, slot: number): Slot {
  const text = storage.getItem(keyOf(slot));
  if (text === null) {
    return { kind: 'empty' };
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    return { kind: 'unreadable', reason: `not JSON (${String(error)})` };
  }
  const save = asSave(parsed);
  return typeof save === 'string'
    ? { kind: 'unreadable', reason: save }
    : { kind: 'held', save };
}

export function readSlots(storage: Storage): Slot[] {
  return Array.from({ length: SLOTS }, (_, slot) => readSlot(storage, slot));
}

export function writeSlot(storage: Storage, slot: number, save: Save): void {
  storage.setItem(keyOf(slot), JSON.stringify(save));
}

export function clearSlot(storage: Storage, slot: number): void {
  storage.removeItem(keyOf(slot));
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** `value` as a save, or the reason it is not one. */
export function asSave(value: unknown): Save | string {
  if (!isRecord(value)) {
    return 'not an object';
  }
  if (value.version !== VERSION) {
    return `version ${String(value.version)}, and this quiz writes ${VERSION}`;
  }
  if (typeof value.session !== 'number') {
    return 'no session';
  }
  if (typeof value.full !== 'boolean') {
    return 'no mode';
  }
  if (typeof value.updated !== 'string') {
    return 'no time';
  }
  const knobs = asKnobs(value.knobs);
  if (typeof knobs === 'string') {
    return knobs;
  }
  if (!Array.isArray(value.answers)) {
    return 'no answers';
  }
  const answers: SavedAnswer[] = [];
  for (const [at, entry] of value.answers.entries()) {
    const answer = asAnswer(entry);
    if (typeof answer === 'string') {
      return `answer ${at + 1}: ${answer}`;
    }
    answers.push(answer);
  }
  return {
    answers,
    full: value.full,
    knobs,
    session: value.session,
    updated: value.updated,
    version: VERSION,
  };
}

/**
 * The knobs are checked against the model's own defaults: the same fields,
 * each of the same type. A save from a quiz whose model has other knobs is
 * not one this quiz can resume, and it says so rather than guessing.
 */
function asKnobs(value: unknown): KnobValues | string {
  if (!isRecord(value)) {
    return 'no knobs';
  }
  const template: Record<string, unknown> = { ...defaultKnobs() };
  for (const [name, example] of Object.entries(template)) {
    if (typeof value[name] !== typeof example) {
      return `knob ${name} is ${typeof value[name]}, and this quiz wants ${typeof example}`;
    }
  }
  const unknown = Object.keys(value).filter((name) => !(name in template));
  if (unknown.length > 0) {
    return `knobs this quiz has none of: ${unknown.join(', ')}`;
  }
  // Every field of the template was found above, of the template's own type,
  // and nothing else was: the object has exactly the shape the constructor
  // takes.
  return Object.fromEntries(
    Object.keys(template).map((name) => [name, value[name]]),
  ) as KnobValues;
}

function asAnswer(value: unknown): SavedAnswer | string {
  if (!isRecord(value)) {
    return 'not an object';
  }
  if (typeof value.item !== 'string') {
    return 'no item';
  }
  if (typeof value.seed !== 'string' || !/^-?\d+$/.test(value.seed)) {
    return 'seed is not an integer';
  }
  if (!isSaid(value.said)) {
    return `said ${JSON.stringify(value.said)}`;
  }
  if (typeof value.reasoning !== 'string') {
    return 'no reasoning';
  }
  if (typeof value.mark !== 'string') {
    return 'no mark';
  }
  return {
    item: value.item,
    mark: value.mark,
    reasoning: value.reasoning,
    said: value.said,
    seed: value.seed,
  };
}
