import {
  type BamlSymbol,
  buildGroups,
  type Side,
  type SymbolMatrix,
  type TreeNode,
  type TsSymbol,
} from './types';

// Going from a reference to the thing it names.
//
// The report addresses symbols by id and the views address rows by index, so
// this is the bridge between them: it locates every symbol in both trees and
// answers, for an id or an address, which row that is.
//
// It no longer decides *what* a name in a signature refers to. That rule — a
// dotted name says where it lives, a bare one walks enclosing scopes, a name
// two declarations answer to points at neither — lives in the pipeline's
// references.baml, which records the resolved ids per symbol. A second
// implementation of it here is what once pointed `Item` in `baml.iter` at an
// unrelated `Item` in `baml.toml`.
//
// What is left is addressing: `resolve` reads back what `addressOf` wrote, so
// a deep link survives a regenerated report. Names rather than indices, because
// indices shift whenever the stdlib does.

/** Where a symbol sits in its own side's tree. */
export interface Ref {
  side: Side;
  group: string;
  /** Symbol indices from the group's top level down to the symbol itself.
   *  Empty when the group header is the destination. */
  path: number[];
}

/** A place the page has been asked to go, and how many times it has been asked.
 *  The counter makes a repeat request observable, so revisiting a row that is
 *  already open still scrolls to it. */
export interface GotoTarget extends Ref {
  nonce: number;
}

export const GOTO_EVENT = 'matrix-goto';

export interface GotoDetail {
  ref: Ref;
  /** The name to put in the URL, when the destination has one that resolves
   *  back to it. Absent for symbols with no spellable path. */
  name: string | null;
}

export function dispatchGoto(source: Element, ref: Ref, name: string | null) {
  source.dispatchEvent(
    new CustomEvent<GotoDetail>(GOTO_EVENT, {
      bubbles: true,
      composed: true,
      detail: { name, ref },
    }),
  );
}

export interface Token {
  /** Whether this is a name that could refer to something, or the syntax around
   *  one. */
  ident: boolean;
  text: string;
}

const IDENT_START = /[A-Za-z_$]/;
const IDENT_PART = /[A-Za-z0-9_$]/;

/**
 * Splits a rendered type into the names it mentions and the syntax between
 * them.
 *
 * A dotted path is one name rather than three — `baml.errors.Io` refers to a
 * class, not to a package and a namespace and a class — and the contents of a
 * string literal type are values, so they are never names at all.
 */
export function tokenize(text: string): Token[] {
  const tokens: Token[] = [];
  let plain = '';
  let at = 0;
  const flush = () => {
    if (plain.length > 0) {
      tokens.push({ ident: false, text: plain });
      plain = '';
    }
  };
  while (at < text.length) {
    const char = text[at] as string;
    if (char === '"' || char === "'") {
      const opened = at;
      at += 1;
      while (at < text.length && text[at] !== char)
        at += text[at] === '\\' ? 2 : 1;
      at = Math.min(at + 1, text.length);
      plain += text.slice(opened, at);
      continue;
    }
    if (!IDENT_START.test(char)) {
      plain += char;
      at += 1;
      continue;
    }
    const started = at;
    while (at < text.length && IDENT_PART.test(text[at] as string)) at += 1;
    while (text[at] === '.' && IDENT_START.test(text[at + 1] ?? '')) {
      at += 1;
      while (at < text.length && IDENT_PART.test(text[at] as string)) at += 1;
    }
    flush();
    tokens.push({ ident: true, text: text.slice(started, at) });
  }
  flush();
  return tokens;
}

/** `null` marks a name that more than one place answers to, which is no better
 *  than an unknown name: there is no single destination to go to. */
type NameMap = Map<string, Ref | null>;

function register(names: NameMap, key: string, ref: Ref) {
  const existing = names.get(key);
  if (existing === undefined) {
    names.set(key, ref);
    return;
  }
  if (existing === null) return;
  if (
    existing.group !== ref.group ||
    existing.path.join() !== ref.path.join()
  ) {
    names.set(key, null);
  }
}

function places(
  side: Side,
  groups: Array<[string, TreeNode[]]>,
): Map<number, Ref> {
  const found = new Map<number, Ref>();
  const walk = (group: string, nodes: TreeNode[], prefix: number[]) => {
    for (const node of nodes) {
      const path = [...prefix, node.index];
      found.set(node.index, { group, path, side });
      walk(group, node.children, path);
    }
  };
  for (const [group, nodes] of groups) walk(group, nodes, []);
  return found;
}

const CACHE = new WeakMap<SymbolMatrix, SymbolIndex>();

/**
 * Both sides' trees, plus every name that leads into them.
 *
 * Built once per report and memoized on it: grouping is deterministic given the
 * report, so the tree the reader sees and the tree references resolve into are
 * necessarily the same one.
 */
export class SymbolIndex {
  static for(matrix: SymbolMatrix): SymbolIndex {
    const cached = CACHE.get(matrix);
    if (cached) return cached;
    const built = new SymbolIndex(matrix);
    CACHE.set(matrix, built);
    return built;
  }

  readonly #groups: Record<Side, Array<[string, TreeNode[]]>>;
  readonly #places: Record<Side, Map<number, Ref>>;
  /** Container names, for addressing a group heading. A TypeScript container
   *  has no row of its own. */
  readonly #groupNames: Record<Side, Set<string>>;
  /** Where each symbol sits, by its id — the report's judgements and
   *  references both name symbols that way. */
  readonly #byId: Record<Side, Map<string, Ref>>;
  /** How each symbol is written, by its id. A reference is recorded as an id
   *  but spelled in a signature as a display: `T:baml.String` is `string`. */
  readonly #displays: Record<Side, Map<string, string>>;
  /** Every symbol by its dotted path, for addressing one from the URL. */
  readonly #symbols: Record<Side, NameMap>;
  /** Each symbol's own dotted path, for writing a destination into the URL. */
  readonly #paths: Record<Side, Map<number, string>>;

  private constructor(matrix: SymbolMatrix) {
    this.#groups = {
      baml: buildGroups('baml', matrix.baml),
      ts: buildGroups('ts', matrix.ts),
    };
    this.#places = {
      baml: places('baml', this.#groups.baml),
      ts: places('ts', this.#groups.ts),
    };
    this.#groupNames = {
      baml: new Set(this.#groups.baml.map(([name]) => name)),
      ts: new Set(this.#groups.ts.map(([name]) => name)),
    };
    this.#byId = { baml: new Map(), ts: new Map() };
    this.#displays = { baml: new Map(), ts: new Map() };
    this.#symbols = { baml: new Map(), ts: new Map() };
    this.#paths = { baml: new Map(), ts: new Map() };
    this.#nameBaml(matrix.baml);
    this.#nameTs(matrix.ts);
  }

  #nameBaml(symbols: BamlSymbol[]) {
    symbols.forEach((symbol, index) => {
      const place = this.#places.baml.get(index);
      if (!place) return;
      this.#byId.baml.set(symbol.id, place);
      this.#displays.baml.set(symbol.id, symbol.display);
      // An impl method's path names its receiver rather than a namespace, so it
      // has no spelling a URL could carry. It is still reachable — as a
      // counterpart, or by opening its type — just not by name.
      if (symbol.symbol.some((step) => typeof step !== 'string')) return;
      const dotted = (symbol.symbol as string[]).join('.');
      this.#paths.baml.set(index, dotted);
      register(this.#symbols.baml, dotted, place);
    });
  }

  #nameTs(symbols: TsSymbol[]) {
    symbols.forEach((symbol, index) => {
      const place = this.#places.ts.get(index);
      if (!place) return;
      this.#byId.ts.set(symbol.id, place);
      this.#displays.ts.set(symbol.id, symbol.display);
      this.#paths.ts.set(index, symbol.id);
      register(this.#symbols.ts, symbol.id, place);
    });
  }

  groups(side: Side): Array<[string, TreeNode[]]> {
    return this.#groups[side];
  }

  /** Where a symbol sits, given its index in the report. */
  place(side: Side, index: number): Ref | null {
    return this.#places[side].get(index) ?? null;
  }

  /**
   * Where a reference points.
   *
   * A reference is usually a symbol id, but the TypeScript side also records
   * bare container names — `Promise`, `URL` — because a container has no row of
   * its own to address. Both resolve here, so a caller need not know which kind
   * it is holding.
   */
  byId(side: Side, id: string): Ref | null {
    return this.#byId[side].get(id) ?? this.#groupRef(side, id);
  }

  /** How a referenced symbol is written, when it is a symbol rather than a
   *  container. */
  displayOf(side: Side, id: string): string | null {
    return this.#displays[side].get(id) ?? null;
  }

  /**
   * What a name from the URL refers to.
   *
   * Only the dotted path a symbol addresses itself by — deep links are written
   * by `addressOf`, so this reads back exactly what that wrote. Resolving a
   * name as a *signature* would spell it is no longer done here: the pipeline
   * resolves references and records the ids, and a second implementation of
   * that rule in the view is how `Item` in `baml.iter` came to point at an
   * unrelated `Item` in `baml.toml`.
   */
  resolve(side: Side, name: string): Ref | null {
    return this.#symbols[side].get(name) ?? this.#groupRef(side, name);
  }

  #groupRef(side: Side, name: string): Ref | null {
    return this.#groupNames[side].has(name)
      ? { group: name, path: [], side }
      : null;
  }

  /**
   * How to write a destination in the URL.
   *
   * Not the name the reference was written with: that one may be relative to
   * the signature it appeared in, and an address has to survive being read back
   * with no such context.
   */
  addressOf(ref: Ref): string | null {
    const last = ref.path.at(-1);
    if (last !== undefined) return this.nameOf(ref.side, last);
    // The group heading, which is the container's own name.
    return this.#groupNames[ref.side].has(ref.group) ? ref.group : null;
  }

  /**
   * The name a place answers to, for the URL — only when writing it down and
   * reading it back lands in the same place.
   */
  nameOf(side: Side, index: number): string | null {
    const dotted = this.#paths[side].get(index);
    const place = this.#places[side].get(index);
    if (dotted === undefined || !place) return null;
    const resolved = this.resolve(side, dotted);
    return resolved &&
      resolved.group === place.group &&
      resolved.path.join() === place.path.join()
      ? dotted
      : null;
  }
}

/** Marks a row as the one just navigated to. */
export function flash(element: HTMLElement) {
  element.classList.remove('goto-flash');
  // Reading layout forces the removal to land before the class goes back on;
  // without it a second visit to the same row would not restart the animation.
  void element.offsetWidth;
  element.classList.add('goto-flash');
}
