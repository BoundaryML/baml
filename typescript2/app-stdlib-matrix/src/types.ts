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
  symbol: string[];
  display: string;
  origin: string;
  signature: string;
  doc: string | null;
  /** The lib that introduced it — `es5`, `es2015`, `dom`, `node`. */
  since: string;
  /** The ids of the symbols this signature names, resolved by the tool. */
  references: string[];
}

/**
 * What was concluded about one BAML symbol, and on what grounds.
 *
 * Both sides are ids, not array indices: indices shift whenever the stdlib
 * does. `verdict` is what is true — `match`, `divergent` (the same operation
 * reached differently), or `none` (nothing on the other side does its job) —
 * and `basis` is why it is believed.
 */
export interface Judgement {
  baml: string;
  ts: string | null;
  verdict: 'match' | 'divergent' | 'none';
  basis: string;
  confidence: string | null;
  reason: string | null;
  divergence: string | null;
  rejected: string[];
  verified: boolean;
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
  /** BAML symbols with a counterpart, exact or divergent. */
  matched: number;
  /** BAML symbols judged to have none. */
  unmatched: number;
  /** BAML symbols nothing has judged yet. */
  unjudged: number;
  ts_unclaimed: number;
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
      const from = bamlIndex.get(judgement.baml);
      if (from !== undefined) push(this.fromBaml, from, judgement);
      // An absence has no other end to index from.
      const to = judgement.ts === null ? undefined : tsIndex.get(judgement.ts);
      if (to !== undefined) push(this.fromTs, to, judgement);
    }
  }

  for(side: Side, index: number): Judgement[] {
    return (side === 'baml' ? this.fromBaml : this.fromTs).get(index) ?? [];
  }
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
 * BAML has three levels and all of them nest: a namespace holds types and free
 * functions, a type holds its members, and an impl's methods hang off the type
 * the impl is written for. Deciding a symbol's group in isolation cannot do
 * that last part — an impl's path names its receiver, not a namespace — so
 * grouping runs once over the whole set with the type index in hand.
 *
 * TypeScript containers have one level, so their members stay flat.
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

  if (side === 'ts') {
    const grouped = new Map<string, TreeNode[]>();
    for (const node of nodes) {
      const key = (node.symbol as TsSymbol).symbol[0] ?? '(unknown)';
      const bucket = grouped.get(key);
      if (bucket) bucket.push(node);
      else grouped.set(key, [node]);
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

  const grouped = new Map<string, TreeNode[]>();
  const add = (key: string, node: TreeNode) => {
    const bucket = grouped.get(key);
    if (bucket) bucket.push(node);
    else grouped.set(key, [node]);
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
