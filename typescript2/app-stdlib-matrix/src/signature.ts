import { html, nothing, type TemplateResult } from 'lit';
import { dispatchGoto, type Ref, SymbolIndex, tokenize } from './navigation';
import type { BamlSymbol, Side, SymbolMatrix, TsSymbol } from './types';

// Rendering a symbol's declaration as one line of highlighted code.
//
// The pieces are already structured — kind, path, generics, args, kwargs,
// return, errors — so this composes spans rather than tokenizing a string.
// Highlighting a rendered signature would mean re-parsing what we took apart.
//
// The leading path is dimmed and only the symbol's own name is bright, so a
// list of forty members in one namespace reads as forty names rather than
// forty near-identical paths.

const KEYWORD = 'text-violet-600 dark:text-violet-400';
const NAME = 'text-zinc-900 dark:text-zinc-100';
const TYPE = 'text-teal-700 dark:text-teal-400';
/** Present but secondary: readable, just not what the eye should land on. */
const MUTED = 'text-zinc-600 dark:text-zinc-400';
const PATH = MUTED;
const PARAM = MUTED;
/** Structure rather than content, so quieter still. */
const PUNCT = 'text-zinc-400 dark:text-zinc-600';

// Type references are links, the way an editor's are: modifier-click goes to
// the declaration. The affordance is deliberately quiet — a signature is text
// first, and a row full of underlined types would be unreadable — so the link
// styling appears only while the modifier is held (see `.type-ref` in
// index.css) and a plain click still belongs to the row.

/**
 * What a type mentioned in one symbol's signature can be resolved against:
 * the symbols that symbol actually names, and nothing else.
 *
 * The view used to resolve names itself — walking enclosing scopes against a
 * global index — which was a second implementation of a rule the pipeline
 * already applies, and the one that pointed `Item` in `baml.iter` at an
 * unrelated `Item` in `baml.toml`. The report now carries the resolved ids per
 * symbol, so resolution here is a lookup in a table of at most a dozen entries.
 *
 * Type parameters need no special handling for the same reason: `T` is not
 * among a symbol's references, so it cannot be linked. The shadow set that used
 * to keep them out existed only to compensate for the global index.
 */
interface Refs {
  index: SymbolIndex;
  side: Side;
  /** Every spelling of a referenced symbol, to the place it addresses. */
  targets: Map<string, Ref>;
}

/**
 * The names a symbol's references can be written as.
 *
 * Two spellings reach the same declaration: its dotted path, and its display.
 * They differ exactly for the companion classes — `T:baml.String` is written
 * `string` in every signature — which is why both are keyed.
 */
function targetsOf(
  symbol: BamlSymbol | TsSymbol,
  side: Side,
  matrix: SymbolMatrix,
): Map<string, Ref> {
  const index = SymbolIndex.for(matrix);
  const targets = new Map<string, Ref>();
  for (const id of symbol.references) {
    const ref = index.byId(side, id);
    if (!ref) continue;
    // The kind prefix is addressing, not spelling: `T:baml.errors.Io` is
    // written `baml.errors.Io`. A TypeScript container is already its own name.
    targets.set(id.includes(':') ? id.slice(id.indexOf(':') + 1) : id, ref);
    const display = index.displayOf(side, id);
    if (display === null) continue;
    targets.set(display, ref);
    // `map<K, V>` is also written by its head alone.
    const head = display.indexOf('<');
    if (head > 0) targets.set(display.slice(0, head), ref);
  }
  return targets;
}

function activate(event: MouseEvent, ref: Ref, address: string | null) {
  event.preventDefault();
  // Unmodified clicks belong to the row, which opens it. Requiring the modifier
  // is the same gesture an editor uses for go-to-definition.
  if (!event.metaKey && !event.ctrlKey) return;
  event.stopPropagation();
  dispatchGoto(event.currentTarget as Element, ref, address);
}

/**
 * Enter on a focused reference follows it.
 *
 * A keyboard has no modifier to hold, and pressing Enter on an anchor
 * dispatches a plain click — which `activate` suppresses on its way to leaving
 * the row's behaviour alone. Without this, a reference is reachable by Tab and
 * does nothing when activated.
 */
function activateByKey(event: KeyboardEvent, ref: Ref, address: string | null) {
  if (event.key !== 'Enter') return;
  event.preventDefault();
  event.stopPropagation();
  dispatchGoto(event.currentTarget as Element, ref, address);
}

/** One name inside a type: a link when this report declares it, plain text
 *  when it does not. A name from outside — another package, a bound, `never` —
 *  has nowhere to go, and an affordance that leads nowhere is worse than none. */
function reference(name: string, refs: Refs): TemplateResult | string {
  const ref = refs.targets.get(name);
  if (!ref) return name;
  const address = refs.index.addressOf(ref);
  return html`<a
    class="type-ref"
    href=${address === null ? nothing : `#${ref.side}/${encodeURIComponent(address)}`}
    @click=${(event: MouseEvent) => activate(event, ref, address)}
    @keydown=${(event: KeyboardEvent) => activateByKey(event, ref, address)}
    >${name}</a
  >`;
}

/** A rendered type, with every name in it resolved as far as it can be. */
function typeText(text: string, refs: Refs): Array<TemplateResult | string> {
  return tokenize(text).map((token) =>
    token.ident ? reference(token.text, refs) : token.text,
  );
}

/**
 * A symbol's fully qualified name, split into the dimmed leading path and the
 * bright final component.
 *
 * A companion class's display is the type it backs (`string`, `T[]`), which is
 * globally addressable and must not be re-qualified back into `baml.String`.
 * Detecting that is exactly the case where the display and the last path
 * segment disagree.
 */
export function qualify(symbol: BamlSymbol): { path: string; name: string } {
  const names = symbol.symbol.filter((s): s is string => typeof s === 'string');
  const display = symbol.display;
  const dot = display.lastIndexOf('.');
  const own = dot < 0 ? display : display.slice(dot + 1);

  // An impl method carries its receiver in the display; leave it whole.
  if (symbol.symbol.length > 0 && typeof symbol.symbol[0] !== 'string') {
    return { name: own, path: display.slice(0, display.length - own.length) };
  }

  const last = names.at(-1);
  const normalized =
    last !== undefined && last !== own && !display.includes('.');
  if (normalized) return { name: display, path: '' };

  // Members show owner-qualified; types and functions show namespace-qualified.
  const qualifiedPath = names.slice(0, -1).join('.');
  const prefix = qualifiedPath.length > 0 ? `${qualifiedPath}.` : '';
  // A normalized owner (`string.char_at`) keeps the owner's own spelling.
  if (dot >= 0) {
    const owner = display.slice(0, dot);
    const ownerNames = names.slice(0, -1);
    const ownerLast = ownerNames.at(-1);
    if (ownerLast !== undefined && ownerLast !== owner) {
      return { name: own, path: `${owner}.` };
    }
  }
  return { name: own, path: prefix };
}

/**
 * A type name that spells its own parameters, with those parameters coloured
 * as types.
 *
 * The container types are displayed the way they are written rather than the
 * way they are declared: `class Array<T>` shows as `T[]`, `class Map<K, V>` as
 * `map<K, V>`. Their declared parameters are therefore already on screen, and
 * appending them again would read `T[]<T>`.
 */
function typeName(name: string): TemplateResult {
  if (name.endsWith('[]')) {
    return html`<span class=${TYPE}>${name.slice(0, -2)}</span
      ><span class=${PUNCT}>[]</span>`;
  }
  const open = name.indexOf('<');
  if (open > 0 && name.endsWith('>')) {
    const base = name.slice(0, open);
    const args = splitArguments(name.slice(open + 1, -1));
    return html`<span class="${NAME} font-medium">${base}</span
      ><span class=${PUNCT}>&lt;</span
      >${args.map(
        (arg, index) =>
          html`${index > 0 ? html`<span class=${PUNCT}>, </span>` : nothing}<span class=${TYPE}
            >${arg.trim()}</span
          >`,
      )}<span class=${PUNCT}>&gt;</span>`;
  }
  return html`<span class="${NAME} font-medium">${name}</span>`;
}

/**
 * A generic argument list, split at the top level only.
 *
 * Splitting on every comma tears a nested display apart: `map<K, map<A, B>>`
 * would render as the arguments `map<A` and `B>>`, which is wrong on screen
 * rather than merely uncoloured. No display in the stdlib nests today; the
 * cost of being right anyway is a depth counter.
 */
function splitArguments(text: string): string[] {
  const args: string[] = [];
  let depth = 0;
  let current = '';
  for (const char of text) {
    if (char === '<') depth += 1;
    else if (char === '>') depth -= 1;
    if (char === ',' && depth === 0) {
      args.push(current);
      current = '';
      continue;
    }
    current += char;
  }
  args.push(current);
  return args;
}

/** True when the display already spells the parameters the declaration lists. */
function spellsOwnGenerics(name: string): boolean {
  return name.endsWith('[]') || (name.includes('<') && name.endsWith('>'));
}

export function bamlSignature(
  symbol: BamlSymbol,
  matrix: SymbolMatrix,
): TemplateResult {
  const { path, name } = qualify(symbol);
  const signature = symbol.signature;
  const declared = spellsOwnGenerics(name) ? [] : symbol.generics;
  const refs: Refs = {
    index: SymbolIndex.for(matrix),
    side: 'baml',
    targets: targetsOf(symbol, 'baml', matrix),
  };
  return html`<span class="font-mono text-[0.82rem] break-words"
    ><span class=${KEYWORD}>${symbol.kind}</span> <span class=${PATH}>${path}</span
    >${typeName(name)}${generics(declared, refs)}${annotation(symbol, refs)}${equated(
      symbol,
      refs,
    )}${
      signature
        ? html`${params(symbol, refs)}${returns(signature.returns, refs)}${throwsOf(
            signature.errors,
            refs,
          )}`
        : nothing
    }</span
  >`;
}

/** The `: <type>` a field is declared with. */
function annotation(symbol: BamlSymbol, refs: Refs) {
  const ty = symbol.ty;
  if (ty === null || ty === undefined) return nothing;
  return html`<span class=${PUNCT}>: </span
    ><span class=${TYPE}>${typeText(ty, refs)}</span>`;
}

/**
 * The `= <type>` a declaration carries, for the two kinds that have one.
 *
 * They are written alike and mean different things. A type alias's right-hand
 * side is what the name *is*; an associated type's is only what an implementor
 * gets for leaving it unbound. The kind keyword ahead of the name is what says
 * which, so the two never need distinguishing here — but they are read from
 * separate fields, because a single field would have to be interpreted.
 */
function equated(symbol: BamlSymbol, refs: Refs) {
  const ty = symbol.resolved ?? symbol.default;
  if (ty === null || ty === undefined) return nothing;
  return html`<span class=${PUNCT}> = </span
    ><span class=${TYPE}>${typeText(ty, refs)}</span>`;
}

function generics(params: string[], refs: Refs) {
  if (params.length === 0) return nothing;
  return html`<span class=${PUNCT}>&lt;</span
    >${params.map((param, index) => {
      const { name, bound } = declaredParam(param);
      return html`${index > 0 ? html`<span class=${PUNCT}>, </span>` : nothing}<span
          class=${TYPE}
          >${name}</span
        >${
          bound === null
            ? nothing
            : html`<span class=${PUNCT}>: </span
              ><span class=${TYPE}>${typeText(bound, refs)}</span>`
        }`;
    })}<span class=${PUNCT}>&gt;</span>`;
}

/**
 * A declared type parameter, split from its bound.
 *
 * The name is a binding rather than a reference — it is what the signature
 * introduces — so only the bound is a link. BAML writes at most one, and no
 * type syntax contains a colon, so the first one separates them.
 */
function declaredParam(param: string): { name: string; bound: string | null } {
  const cut = param.indexOf(':');
  if (cut < 0) return { bound: null, name: param.trim() };
  return {
    bound: param.slice(cut + 1).trim(),
    name: param.slice(0, cut).trim(),
  };
}

function params(symbol: BamlSymbol, refs: Refs) {
  const signature = symbol.signature;
  if (!signature) return nothing;
  const positional = signature.args;
  const named = Object.entries(signature.kwargs);
  const all = [
    ...positional.map(({ name, ty }) => ({ name, optional: false, ty })),
    ...named.map(([name, ty]) => ({ name, optional: true, ty })),
  ];
  return html`${generics(signature.generic_params, refs)}<span class=${PUNCT}>(</span
    >${all.map(
      (
        param,
        index,
      ) => html`${index > 0 ? html`<span class=${PUNCT}>, </span>` : nothing}<span
          class=${PARAM}
          >${param.name}</span
        ><span class=${PUNCT}>: </span><span class=${TYPE}>${typeText(param.ty, refs)}</span
        >${param.optional ? html`<span class=${PUNCT}> = …</span>` : nothing}`,
    )}<span class=${PUNCT}>)</span>`;
}

function returns(ty: string, refs: Refs) {
  if (ty === 'void') return nothing;
  return html`<span class=${PUNCT}> -&gt; </span><span class=${TYPE}>${typeText(ty, refs)}</span>`;
}

function throwsOf(errors: string, refs: Refs) {
  if (errors === 'never') return nothing;
  return html` <span class=${KEYWORD}>throws</span>
    <span class=${TYPE}>${typeText(errors, refs)}</span>`;
}

/**
 * TypeScript's signature is printed text from the `.d.ts`, not a structure, so
 * it cannot be recomposed the way BAML's can. It can still be split: the
 * printed form contains the member's own name, so lifting the name out leaves
 * the modifiers before it and the parameters and type after it.
 *
 * Splitting matters because the name is shown separately — without it the row
 * reads `ArrayBuffer.byteLength readonly byteLength: number`, saying everything
 * twice.
 */
function tsParts(symbol: TsSymbol): { modifiers: string; rest: string } {
  const printed = symbol.signature.split('\n')[0] ?? '';
  const name = symbol.symbol.at(-1) ?? '';
  // Synthesized names for unnamed members — `(new)`, `(call)`, `(index)` —
  // appear in the source without their parentheses.
  const bare =
    name.startsWith('(') && name.endsWith(')') ? name.slice(1, -1) : name;
  const at = printed.indexOf(bare);
  if (bare.length === 0 || at < 0) return { modifiers: '', rest: printed };
  return {
    modifiers: printed.slice(0, at).trim(),
    rest: printed.slice(at + bare.length),
  };
}

/** The word a TypeScript member reads as: its own modifier when it has one. */
function tsKeyword(symbol: TsSymbol, modifiers: string): string {
  if (modifiers.length > 0) return modifiers;
  return symbol.origin === 'property' ? 'property' : 'function';
}

export function tsSignature(
  symbol: TsSymbol,
  matrix: SymbolMatrix,
): TemplateResult {
  // The name comes from the path, never from splitting the display on its last
  // dot: a computed name like `[Symbol.species]` contains dots of its own, and
  // splitting on text would cut it into `ArrayBuffer.[Symbol.` and `species]`.
  const steps = symbol.symbol;
  const name = steps.at(-1) ?? symbol.display;
  // The owner comes off the display by length rather than from the path, for
  // two reasons. The path's first step is the module, which groups but is not
  // part of how a symbol is written — `(web).DOMException.` is not an address.
  // And the path has no `prototype` step, so the static and instance sides of a
  // container render identically from it: `DOMException.ABORT_ERR` exists on
  // both, and the two rows were indistinguishable. The display already carries
  // TypeScript's own spelling of both facts.
  const container = symbol.display.endsWith(name)
    ? symbol.display.slice(0, symbol.display.length - name.length)
    : '';
  const { modifiers, rest } = tsParts(symbol);
  // No scope walk on this side: a TypeScript signature is printed text, so its
  // parameter names sit in the same string as its types, and resolving them
  // against the enclosing container would link `message` in `Error(message?:
  // string)` to the `Error.message` property.
  const refs: Refs = {
    index: SymbolIndex.for(matrix),
    side: 'ts',
    targets: targetsOf(symbol, 'ts', matrix),
  };
  return html`<span class="font-mono text-[0.82rem] break-words"
    ><span class=${KEYWORD}>${tsKeyword(symbol, modifiers)}</span> <span class=${PATH}
      >${container}</span
    ><span class="${NAME} font-medium">${name}</span
    ><span class=${TYPE}>${typeText(rest, refs)}</span></span
  >`;
}
