// The shape of `tools/stdlib-matrix`'s JSON report.
//
// Declared structurally rather than generated: the report is produced by a BAML
// program and there is no shared schema yet. Everything is optional-tolerant —
// a field the generator stops emitting should blank a cell, never break the
// render.

/** A step of a BAML symbol path. Most are names; an impl method's first step
 *  carries the receiver and the interface it is reached through. */
export type PathStep = string | ImplStep;

export interface ImplStep {
  base: string;
  interface: string[];
}

export interface SignatureArg {
  name: string;
  ty: string;
}

export interface SymbolSignature {
  display: string;
  generic_params: string[];
  args: SignatureArg[];
  /** Default-valued parameters, which BAML makes named-only. */
  kwargs: Record<string, string>;
  returns: string;
  /** The effective error contract; `"never"` when the call cannot fail. */
  errors: string;
}

export interface BamlSymbol {
  /** The export's own id for this declaration: stable across runs and unique
   *  across the report. Judgements are recorded against it. */
  id: string;
  /** The declaration an impl entry re-lists, when it has an id of its own. */
  declared_by: string | null;
  symbol: PathStep[];
  display: string;
  /** What it is, as declared: `class`, `enum`, `interface`, `type`,
   *  `associated type`, `function`, `field`, `variant`. */
  kind: string;
  /** How it is reached: `free`, `method`, `static_method`, `impl_method`,
   *  `type`, `field`, `variant`, `assoc_type`. */
  origin: string;
  /** Declared type parameters, as written. */
  generics: string[];
  /** The type a field is declared to hold. */
  ty: string | null;
  /** What a type alias names. An alias *is* its right-hand side; only aliases
   *  have one. */
  resolved: string | null;
  /** What an associated type falls back to when an implementor leaves it
   *  unbound — a fallback, not a definition, and most have none. */
  default: string | null;
  /** The ids of the symbols this declaration names, resolved by the tool. */
  references: string[];
  signature: SymbolSignature | null;
  doc: string | null;
}

export interface TsSymbol {
  /** This symbol's address, as TypeScript writes it: the instance side is
   *  reached through `prototype`, the static side on the constructor. */
  id: string;
  /** `[module, container]`, `[module, function]`, or
   *  `[module, container, member]` — three levels, the way a reader reaches it.
   *  The module groups and is not part of how the symbol is written. */
  symbol: string[];
  display: string;
  /** What it is, as declared: `class`, `interface`, `namespace`, `function`,
   *  `method`, `static method`, `property`. */
  kind: string;
  /** How it is reached, and how the view nests it: `container` for a type,
   *  `free` for something reached on the module, and `method` /
   *  `static_method` / `property` for a container's members. */
  origin: string;
  signature: string;
  doc: string | null;
  /** The lib that introduced it — `es5`, `es2015`, `dom`, `node`. */
  since: string;
  /** The ids of the symbols this signature names, resolved by the tool. */
  references: string[];
}

/** Two snippets showing the same job done in each language. */
export interface CodeExample {
  typescript: string;
  baml: string;
  note: string | null;
}

/**
 * What was concluded about one TypeScript symbol, and on what grounds.
 *
 * Keyed on the TypeScript side, because that is the side a reader arrives from:
 * they know how to do something in TypeScript and want to know how it is done
 * in BAML. `baml` is a list — "how you would do this" is not always one symbol,
 * and zero entries means nothing does the job.
 *
 * Every side is an id, not an array index: indices shift whenever either stdlib
 * does. `verdict` is what is true — `match`, `divergent` (the same operation
 * reached differently), `unnecessary` (BAML answers the question in the
 * language, so the API does not arise), or `none` (nothing in BAML does its
 * job) — and `basis` is why it is believed.
 */
export interface Judgement {
  ts: string;
  baml: string[];
  verdict: 'match' | 'divergent' | 'unnecessary' | 'none';
  basis: string;
  confidence: string | null;
  reason: string | null;
  divergence: string | null;
  rejected: string[];
  verified: boolean;
  example: CodeExample | null;
}

/** A call that did not come back, recorded rather than swallowed. */
export interface PassFailure {
  pass: string;
  subject: string;
  reason: string;
}

export interface MatrixProvenance {
  baml_surface_sha256: string;
  export_format_version: number;
  typescript_version: string;
  types_node_version: string | null;
}

export interface MatrixCounts {
  baml_symbols: number;
  ts_symbols: number;
  judgements: number;
  /** TypeScript symbols with a BAML counterpart, exact or divergent. */
  matched: number;
  /** TypeScript symbols BAML makes unnecessary. Neither a correspondence nor a
   *  gap, so counted apart from both. */
  unnecessary: number;
  /** TypeScript symbols judged to have none. */
  unmatched: number;
  /** TypeScript symbols nothing has judged yet. */
  unjudged: number;
  /** BAML symbols no judgement names. Not the same claim as `unmatched`:
   *  nothing asked whether these have a counterpart, so it counts silence
   *  rather than a finding, and the two must not be added up. */
  baml_unclaimed: number;
}

export interface SymbolMatrix {
  format_version: number;
  provenance: MatrixProvenance;
  baml: BamlSymbol[];
  ts: TsSymbol[];
  judgements: Judgement[];
  failures: PassFailure[];
  counts: MatrixCounts;
}

export type Side = 'baml' | 'ts';

/**
 * Judgements indexed from both ends, so either view can answer "what was
 * concluded about this symbol" without scanning.
 *
 * The report keys on ids while the views address rows by array index, so this
 * is also where the two meet: the id maps are built once from the symbol
 * arrays, and every lookup after that is by index.
 */
export class Links {
  readonly fromBaml = new Map<number, Judgement[]>();
  readonly fromTs = new Map<number, Judgement[]>();

  constructor(matrix: SymbolMatrix) {
    const bamlIndex = indexById(matrix.baml);
    const tsIndex = indexById(matrix.ts);
    for (const judgement of matrix.judgements ?? []) {
      const from = tsIndex.get(judgement.ts);
      if (from !== undefined) push(this.fromTs, from, judgement);
      // A judgement may name several BAML symbols, or — when it records an
      // absence — none, in which case there is no other end to index from.
      for (const id of judgement.baml) {
        const to = bamlIndex.get(id);
        if (to !== undefined) push(this.fromBaml, to, judgement);
      }
    }
  }

  for(side: Side, index: number): Judgement[] {
    return (side === 'baml' ? this.fromBaml : this.fromTs).get(index) ?? [];
  }
}

/**
 * What the run concluded about one symbol, reduced to the states the view
 * distinguishes.
 *
 * The two sides do not mean the same thing by `unjudged`, and the legend says
 * so. On the TypeScript side it is "nothing has looked at this yet", against a
 * `none` that means "something looked and found nothing" — a finding. A BAML
 * symbol only ever indexes from judgements that *name* it, so it never reaches
 * `none`: `unjudged` there means no judgement happened to name it, which is
 * silence rather than a conclusion.
 *
 * Divergence wins over an exact match when a symbol has both. A reader scanning
 * for the places the two libraries disagree should not have to open a row to
 * find them.
 */
export type SymbolState =
  | 'match'
  | 'divergent'
  | 'unnecessary'
  | 'none'
  | 'unjudged';

/**
 * The strongest thing said about a symbol, when more than one judgement speaks
 * for it. The same order `judgement_rank` applies in report.baml, so the square
 * and the report's own tally cannot disagree.
 */
const RANK: Record<string, number> = {
  divergent: 4,
  match: 3,
  none: 1,
  unnecessary: 2,
};

export function stateOf(links: Links, side: Side, index: number): SymbolState {
  let strongest: SymbolState = 'unjudged';
  let rank = 0;
  for (const judgement of links.for(side, index)) {
    const candidate = RANK[judgement.verdict] ?? 0;
    if (candidate > rank) {
      rank = candidate;
      strongest = judgement.verdict;
    }
  }
  // `unnecessary` is a statement about a TypeScript API, not about a BAML
  // symbol. A BAML symbol only reaches it by being named as how you would do
  // the thing anyway — `Date.prototype.setUTCHours` is unnecessary because
  // values are immutable, and `ZonedDateTime.from_components` is what you use
  // instead. From that side it is simply claimed, so it reads as one. Folded
  // here rather than in the renderers so the square and the bar cannot disagree.
  return side === 'baml' && strongest === 'unnecessary' ? 'match' : strongest;
}

/** A tree's size, counting every descendant — what a group's bar is drawn to. */
export function countNodes(nodes: TreeNode[]): number {
  let total = 0;
  for (const node of nodes) total += 1 + countNodes(node.children);
  return total;
}

function indexById(symbols: Array<{ id: string }>): Map<string, number> {
  const byId = new Map<string, number>();
  symbols.forEach((symbol, index) => byId.set(symbol.id, index));
  return byId;
}

function push(map: Map<number, Judgement[]>, key: number, link: Judgement) {
  const bucket = map.get(key);
  if (bucket) bucket.push(link);
  else map.set(key, [link]);
}

/** A symbol plus whatever is declared on it. */
export interface TreeNode {
  index: number;
  symbol: BamlSymbol | TsSymbol;
  children: TreeNode[];
}

const MEMBER_ORIGINS = new Set([
  'method',
  'static_method',
  'field',
  'variant',
  'assoc_type',
]);

/** Declared on a type, rather than directly in a namespace. */
function isMember(symbol: BamlSymbol): boolean {
  return MEMBER_ORIGINS.has(symbol.origin);
}

/**
 * The container a TypeScript path names, as a map key.
 *
 * Module-qualified, because `Server` is declared by `http`, `net`, and `tls`:
 * a bare name would hang all three sets of members off whichever container came
 * first. The separator is a unit separator rather than a dot, since both halves
 * can contain dots of their own.
 */
function containerKey(path: string[]): string {
  return `${path[0] ?? ''}\u001f${path[1] ?? ''}`;
}

function pathNames(symbol: BamlSymbol): string[] {
  return symbol.symbol.filter(
    (step): step is string => typeof step === 'string',
  );
}

function implStepOf(symbol: BamlSymbol): ImplStep | undefined {
  const head = symbol.symbol[0];
  return head && typeof head !== 'string' ? head : undefined;
}

/**
 * The receiver an impl is written for, reduced to the type that owns it.
 *
 * An impl records its receiver as written — `baml.iter.ArrayIterator<T>`,
 * `float[]`, `map<string, V>`, `bigint` — while types are indexed by dotted
 * path (`baml.iter.ArrayIterator`) or display (`T[]`). Reducing the receiver
 * to its head constructor reconciles the two: `float[]` and `T[]` are both the
 * array type, `map<string, V>` and `map<K, V>` both the map type, and `T[]` is
 * simply how that type is spelled generically.
 *
 * A specialized receiver still files under its head. `mean` is declared for
 * `float[]` alone, but it belongs to the array type, and the member's own
 * display — `(float[] as baml.FloatStats).mean` — already says which arrays
 * have it, so nesting loses nothing.
 */
function implBaseKey(base: string): string {
  const trimmed = base.trim();
  if (trimmed.endsWith('[]')) return 'T[]';
  if (trimmed.startsWith('map<')) return 'map<K, V>';
  const cut = trimmed.indexOf('<');
  return (cut < 0 ? trimmed : trimmed.slice(0, cut)).trim();
}

/**
 * Groups one side into its own language's hierarchy.
 *
 * Both languages have three levels and both nest. In BAML a namespace holds
 * types and free functions, a type holds its members, and an impl's methods
 * hang off the type the impl is written for. In TypeScript a module — the
 * globals, the web platform, or one node module — holds containers and free
 * functions, and a container holds its members.
 *
 * Deciding a symbol's group in isolation cannot do the nesting: an impl's path
 * names its receiver rather than a namespace, and a TypeScript member names its
 * container without saying which record that is. So grouping runs once over the
 * whole set, with the containers indexed first.
 */
export function buildGroups(
  side: Side,
  symbols: Array<BamlSymbol | TsSymbol>,
): Array<[string, TreeNode[]]> {
  const nodes: TreeNode[] = symbols.map((symbol, index) => ({
    children: [],
    index,
    symbol,
  }));

  const grouped = new Map<string, TreeNode[]>();
  const add = (key: string, node: TreeNode) => {
    const bucket = grouped.get(key);
    if (bucket) bucket.push(node);
    else grouped.set(key, [node]);
  };

  if (side === 'ts') {
    // Containers are the attachment point for their members. Keyed by module
    // as well as name: `Server` is declared by `http`, `net`, and `tls`, and a
    // bare name would hang all three sets of members off whichever came first.
    const containers = new Map<string, TreeNode>();
    for (const node of nodes) {
      const symbol = node.symbol as TsSymbol;
      if (symbol.origin === 'container') {
        containers.set(containerKey(symbol.symbol), node);
      }
    }
    for (const node of nodes) {
      const symbol = node.symbol as TsSymbol;
      const module = symbol.symbol[0] ?? '(unknown)';
      const parent =
        symbol.symbol.length >= 3
          ? containers.get(containerKey(symbol.symbol))
          : undefined;
      if (parent) parent.children.push(node);
      else add(module, node);
    }
    return sorted(grouped);
  }

  // Types are the attachment point for both members and impl methods.
  const byPath = new Map<string, TreeNode>();
  const byDisplay = new Map<string, TreeNode>();
  for (const node of nodes) {
    const symbol = node.symbol as BamlSymbol;
    if (symbol.origin === 'type') {
      byPath.set(pathNames(symbol).join('.'), node);
      byDisplay.set(symbol.display, node);
    }
  }

  const typeOf = (symbol: BamlSymbol): TreeNode | undefined => {
    const impl = implStepOf(symbol);
    if (impl) {
      const key = implBaseKey(impl.base);
      return byPath.get(key) ?? byDisplay.get(key);
    }
    return isMember(symbol)
      ? byPath.get(pathNames(symbol).slice(0, -1).join('.'))
      : undefined;
  };

  for (const node of nodes) {
    const symbol = node.symbol as BamlSymbol;
    const parent = typeOf(symbol);
    if (parent) {
      parent.children.push(node);
      continue;
    }
    const impl = implStepOf(symbol);
    if (impl) {
      // The receiver is a type this package does not declare — a blanket impl
      // over a type variable, say. Group it by the impl itself.
      add(`${impl.base} as ${impl.interface.join('.')}`, node);
      continue;
    }
    const names = pathNames(symbol);
    const depth = names.length - (isMember(symbol) ? 2 : 1);
    add(
      depth > 0 ? names.slice(0, depth).join('.') : (names[0] ?? '(root)'),
      node,
    );
  }

  return sorted(grouped);
}

function sorted(grouped: Map<string, TreeNode[]>): Array<[string, TreeNode[]]> {
  return [...grouped.entries()].sort(([a], [b]) => a.localeCompare(b));
}
