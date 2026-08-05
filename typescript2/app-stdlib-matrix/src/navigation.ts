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
// A signature spells its types as text — `baml.fs.DirEntry[]`, `map<string,
// unknown>` — while the report addresses symbols by array index. This is the
// bridge: it names every place in both trees, so a type mentioned in one row
// can be found and opened wherever it is actually declared.
//
// Resolution is by name rather than by index, because a name survives a
// regenerated report and can therefore live in the URL. Indices cannot: they
// shift whenever the stdlib does.

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
  /** What a type reference can land on: declarations, by every name they can be
   *  written under. Members are deliberately absent — a type position names a
   *  type, and letting it reach a method is how a name match becomes a lie. */
  readonly #types: Record<Side, NameMap>;
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
    this.#types = { baml: new Map(), ts: new Map() };
    this.#symbols = { baml: new Map(), ts: new Map() };
    this.#paths = { baml: new Map(), ts: new Map() };
    this.#nameBaml(matrix.baml);
    this.#nameTs(matrix.ts);
  }

  #nameBaml(symbols: BamlSymbol[]) {
    symbols.forEach((symbol, index) => {
      const place = this.#places.baml.get(index);
      // An impl method's path names its receiver rather than a namespace, so it
      // has no spelling a signature could mention. It is still reachable — as a
      // counterpart, or by opening its type — just not by name.
      if (!place || symbol.symbol.some((step) => typeof step !== 'string'))
        return;
      const names = symbol.symbol as string[];
      const dotted = names.join('.');
      this.#paths.baml.set(index, dotted);
      register(this.#symbols.baml, dotted, place);
      if (symbol.origin !== 'type') return;
      register(this.#types.baml, dotted, place);
      if (symbol.display === names.at(-1)) return;
      // A companion class is displayed as the type it backs — `string`, `T[]`,
      // `map<K, V>` — and that spelling is what signatures use, so it addresses
      // globally. `map<K, V>` is also referred to by its head alone.
      register(this.#types.baml, symbol.display, place);
      const head = symbol.display.indexOf('<');
      if (head > 0)
        register(this.#types.baml, symbol.display.slice(0, head), place);
    });
  }

  #nameTs(symbols: TsSymbol[]) {
    // A TypeScript container has no row of its own — it is the group — so its
    // name leads to the heading.
    for (const [group] of this.#groups.ts) {
      register(this.#types.ts, group, { group, path: [], side: 'ts' });
    }
    symbols.forEach((symbol, index) => {
      const place = this.#places.ts.get(index);
      if (!place) return;
      const dotted = symbol.symbol.join('.');
      this.#paths.ts.set(index, dotted);
      register(this.#symbols.ts, dotted, place);
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
   * What a name written in a signature — or in the URL — refers to.
   *
   * A signature may spell a type relative to where it was declared, so an
   * unqualified name is tried against each enclosing scope, innermost first,
   * before being taken as global: the same walk the compiler does. Nothing else
   * makes an unqualified name match, because matching bare names across the
   * whole surface pairs `Item` in `baml.iter` with an unrelated `Item` in
   * `baml.toml` — the failure the report's own owner-gate exists to prevent.
   *
   * A name that carries a dot has already said where it lives, so it may reach
   * anything, member or type: `Symbol.iterator` names a property, and that
   * property is where the reader wants to land.
   */
  resolve(side: Side, name: string, scope: string[] = []): Ref | null {
    const types = this.#types[side];
    for (let depth = scope.length; depth > 0; depth -= 1) {
      const found = types.get(`${scope.slice(0, depth).join('.')}.${name}`);
      if (found) return found;
    }
    const direct = types.get(name);
    if (direct) return direct;
    return name.includes('.') ? (this.#symbols[side].get(name) ?? null) : null;
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
    return this.#types[ref.side].has(ref.group) ? ref.group : null;
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
