// Save slots, like a game's.
//
// A slot holds what the page keeps of a sitting and nothing the sitting can
// work out for itself: the session, the knobs, and what was said about each
// case. The profile is rebuilt by replaying those on load, so a save can
// never hold a profile its answers do not lead to; and a save made against a
// quiz whose bank or model has since changed fails to replay and says so,
// instead of quietly resuming with beliefs its answers no longer support.

import {
  defaultKnobs,
  isSaid,
  type KnobValues,
  knobsFault,
  type Said,
} from './quiz';

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
  /** Who marked it: a model's name, or absent when the learner did. Saves
   * from before the grader have no such field, and read as self-marked. */
  marked_by?: string;
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

/**
 * A place the page keeps strings between one moment and the next.
 *
 * Deliberately narrower than the browser's `Storage`: no `clear`, no `key`,
 * no `length`. Browser storage belongs to the origin, and this site shares
 * its origin with every other tool published beside it, so anything that
 * walks or empties a store walks or empties theirs too. A store that cannot
 * enumerate cannot do either.
 */
export interface Store {
  /**
   * Whether what is written here outlives this visit. False when the browser
   * gave no store at all, or stopped accepting writes part-way through; the
   * page carries on in memory either way.
   */
  readonly persisting: boolean;
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/**
 * One of the browser's stores, made to never throw and never to lose a write
 * it took.
 *
 * Opening a browser store can throw — a browser told to block storage throws
 * on the property read itself, and Safari in private mode hands back a store
 * that throws on the first write — and a write can throw later, once the
 * origin's quota is full. Neither is a reason to end a sitting, so both
 * degrade: writes the browser will not take are kept in memory for the rest
 * of the visit, and reads look there first.
 *
 * Removal still reaches the browser after that, because removing frees space
 * rather than taking it. Were it only removed from memory, a slot reset after
 * the quota filled would disappear for this visit and come back on the next.
 * Should even that fail, the memory entry is kept as a removal, so a record
 * the browser still holds cannot show through one this visit deleted.
 */
function layered(open: () => Storage): Store {
  let under: Storage | null;
  try {
    const held = open();
    const probe = 'type-quiz/probe';
    held.setItem(probe, '1');
    held.removeItem(probe);
    under = held;
  } catch {
    under = null;
  }
  // `null` is a removal the browser would not take.
  const over = new Map<string, string | null>();
  let persisting = under !== null;
  return {
    getItem: (key) => {
      if (over.has(key)) {
        return over.get(key) ?? null;
      }
      try {
        return under?.getItem(key) ?? null;
      } catch {
        return null;
      }
    },
    get persisting() {
      return persisting;
    },
    removeItem: (key) => {
      try {
        under?.removeItem(key);
        over.delete(key);
      } catch {
        over.set(key, null);
      }
    },
    setItem: (key, value) => {
      if (persisting && under !== null) {
        try {
          under.setItem(key, value);
          over.delete(key);
          return;
        } catch {
          // On every browser that throws here, the origin's quota is full.
          persisting = false;
        }
      }
      over.set(key, value);
    },
  };
}

let sittings: Store | null = null;
let keys: Store | null = null;

/**
 * Where sittings are kept: this browser's local storage, degrading to memory.
 * The same store on every call, so a degradation one caller meets is the one
 * every other caller reads through.
 */
export function storageOf(): Store {
  sittings ??= layered(() => window.localStorage);
  return sittings;
}

/**
 * Where a key is kept: this TAB, and only while it is open, degrading to
 * memory. The same store on every call: were a fresh one made per call, a key
 * saved through one would be missing from the next, and a configured judge
 * would silently become marking by hand.
 *
 * Storage is scoped to an origin and not to a path, and this site shares
 * `<org>.github.io` with every other project page the organisation publishes.
 * A key in local storage is therefore readable by any of them, at any time,
 * for as long as it sits there. Session storage is scoped to the tab as well,
 * so a sibling page opened in another tab cannot reach it and nothing is left
 * behind when this one closes. It costs the learner one paste per session.
 *
 * This is a reduction and not a fix: a page on this origin, in this tab,
 * could still read it. Only a hostname of our own would end that.
 */
export function keyStoreOf(): Store {
  keys ??= layered(() => window.sessionStorage);
  return keys;
}

export function readSlot(storage: Store, slot: number): Slot {
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

export function readSlots(storage: Store): Slot[] {
  return Array.from({ length: SLOTS }, (_, slot) => readSlot(storage, slot));
}

export function writeSlot(storage: Store, slot: number, save: Save): void {
  storage.setItem(keyOf(slot), JSON.stringify(save));
}

export function clearSlot(storage: Store, slot: number): void {
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
  const held = Object.fromEntries(
    Object.keys(template).map((name) => [name, value[name]]),
  ) as KnobValues;
  // Shape is not enough. Two knobs have a domain outside which the engine's
  // arithmetic panics rather than answering, and a save is the one place
  // knobs arrive from outside this page's own controls. The engine states
  // those bounds, so this asks rather than restating them.
  const fault = knobsFault(held);
  if (fault !== null) {
    return fault;
  }
  return held;
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
  // Absent in saves from before the grader, which were all self-marked.
  if (value.marked_by !== undefined && typeof value.marked_by !== 'string') {
    return 'marked_by is not a name';
  }
  return {
    item: value.item,
    mark: value.mark,
    ...(typeof value.marked_by === 'string' && value.marked_by !== ''
      ? { marked_by: value.marked_by }
      : {}),
    reasoning: value.reasoning,
    said: value.said,
    seed: value.seed,
  };
}
