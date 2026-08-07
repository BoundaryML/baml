import {
  html,
  LitElement,
  nothing,
  type PropertyValues,
  type TemplateResult,
} from 'lit';
import {
  dispatchGoto,
  flash,
  type GotoTarget,
  SymbolIndex,
} from '../navigation';
import { bamlSignature, tsSignature } from '../signature';
import type {
  BamlSymbol,
  Judgement,
  Links,
  Side,
  SymbolMatrix,
  TreeNode,
  TsSymbol,
} from '../types';

// One symbol row, seen from whichever side the reader is viewing.
//
// The swatch answers "does this exist in the other language": striped when it
// has a counterpart, otherwise the viewing language's own colour.
//
// A row is only handed a `target` when it is on the path to one — its parent
// passes it down to the one child that leads there — so holding it at all means
// "open, and if the path ends here, this is the destination".

/**
 * What the square says, before anything is opened.
 *
 * Four states, because the run distinguishes four. Striped is present in both
 * and reached the same way; split is present in both and reached differently;
 * solid is examined and found to have no counterpart; hollow is nothing has
 * looked yet. Hollow reads as empty and so as unknown, solid as a settled
 * finding — which is the distinction a reader most needs and the one the view
 * could not previously make at all.
 */
/** A heading over one part of an opened row. */
const SECTION = (label: string) => html`<h4
  class="mt-3 mb-1 text-[0.68rem] font-semibold tracking-wider text-zinc-500 uppercase"
>
  ${label}
</h4>`;

/** What a judgement rests on, said in words rather than in the report's slug. */
const BASIS: Record<string, string> = {
  model: 'judged by a model',
  name: 'names and owners agree',
  'no-candidates': 'nothing comparable exists to consider',
};

const SWATCH = {
  baml: ['swatch-baml', 'no TypeScript counterpart'],
  both: ['swatch-both', 'present in both'],
  divergent: ['swatch-divergent', 'present in both, reached differently'],
  ts: ['swatch-ts', 'no BAML counterpart'],
  unjudgedBaml: ['swatch-unjudged-baml', 'not yet judged'],
  unjudgedTs: ['swatch-unjudged-ts', 'not yet judged'],
} as const;

export class MatrixSymbolElement extends LitElement {
  static properties = {
    depth: { type: Number },
    links: { attribute: false },
    matrix: { attribute: false },
    node: { attribute: false },
    open: { state: true, type: Boolean },
    side: {},
    target: { attribute: false },
  };

  declare side: Side;
  declare node: TreeNode;
  declare links: Links;
  declare matrix: SymbolMatrix;
  declare target: GotoTarget | null;
  declare depth: number;
  declare open: boolean;
  /** The last request this row scrolled for, so a repeat request scrolls again
   *  but a re-render for any other reason does not. */
  #focused = -1;

  constructor() {
    super();
    this.side = 'baml';
    this.target = null;
    this.depth = 0;
    this.open = false;
  }

  protected willUpdate(changed: PropertyValues) {
    if (changed.has('target') && this.target) this.open = true;
  }

  protected updated() {
    const target = this.target;
    if (!target || target.path.length !== this.depth + 1) return;
    if (target.nonce === this.#focused) return;
    this.#focused = target.nonce;
    this.scrollIntoView({ behavior: 'smooth', block: 'center' });
    flash(this);
  }

  /** The target, but only for the child that leads to it. */
  private targetFor(child: TreeNode): GotoTarget | null {
    return this.target?.path[this.depth + 1] === child.index
      ? this.target
      : null;
  }

  private toggle() {
    // A row with nothing behind it is static: no arrow, no hover, nothing to
    // open. The arrow's glyph is still rendered, hidden, so that rows with and
    // without one line up.
    if (!this.expandable) return;
    this.open = !this.open;
  }

  private key(event: KeyboardEvent) {
    if (event.key !== 'Enter' && event.key !== ' ') return;
    event.preventDefault();
    this.toggle();
  }

  private get symbol(): BamlSymbol | TsSymbol {
    return this.node.symbol;
  }

  /** Judgements about this symbol, not about its children. */
  private get own(): Judgement[] {
    return this.links.for(this.side, this.node.index);
  }

  /** The judgements that pair it with something, as opposed to recording an
   *  absence. */
  private get counterparts(): Judgement[] {
    return this.own.filter(
      (judgement) =>
        judgement.verdict === 'match' || judgement.verdict === 'divergent',
    );
  }

  // Light DOM: Tailwind's stylesheet cannot cross a shadow boundary, and this
  // page has no host to be isolated from.
  protected createRenderRoot() {
    return this;
  }

  private get swatch(): readonly [string, string] {
    const counterparts = this.counterparts;
    if (counterparts.length > 0) {
      // Any divergence colours the square: a reader scanning for the places the
      // two libraries disagree should not have to open a row to find them.
      return counterparts.some((judgement) => judgement.verdict === 'divergent')
        ? SWATCH.divergent
        : SWATCH.both;
    }
    // An absence is only recorded against the BAML symbol — it carries
    // `ts: null` and never indexes from the TypeScript end — so that side has
    // no unjudged state to show. After a sweep, unclaimed is the finding.
    if (this.side === 'ts') return SWATCH.ts;
    return this.own.length > 0 ? SWATCH.baml : SWATCH.unjudgedBaml;
  }

  private signatureOf(symbol: BamlSymbol | TsSymbol, side: Side): string {
    return side === 'ts'
      ? ((symbol as TsSymbol).signature.split('\n')[0] ?? '')
      : ((symbol as BamlSymbol).signature?.display ?? '');
  }

  /** What the row can say that its own title does not already. The kind and the
   *  error contract are both in the signature, so only TypeScript's lib version
   *  is left. */
  private get facts(): string {
    return this.side === 'ts' ? `since ${(this.symbol as TsSymbol).since}` : '';
  }

  /** Whether opening the row would show anything at all. */
  private get expandable(): boolean {
    return (
      (this.symbol.doc?.length ?? 0) > 0 ||
      this.facts.length > 0 ||
      this.own.length > 0 ||
      this.node.children.length > 0
    );
  }

  render() {
    const [swatchClass, swatchLabel] = this.swatch;
    const expandable = this.expandable;
    return html`
      <div
        role=${expandable ? 'button' : nothing}
        tabindex=${expandable ? '0' : nothing}
        class="grid cursor-default grid-cols-[auto_1rem_1fr] items-baseline gap-2.5 py-1.5
               pr-3 text-left focus-visible:outline-2 focus-visible:-outline-offset-2
               focus-visible:outline-blue-500 ${
                 expandable ? 'hover:bg-zinc-100 dark:hover:bg-zinc-900' : ''
               }"
        aria-expanded=${expandable ? this.open : nothing}
        style=${`padding-left: ${0.75 + this.depth * 1.25}rem`}
        @click=${this.toggle}
        @keydown=${this.key}
      >
        <span
          aria-hidden="true"
          class="self-center text-[0.7rem] text-zinc-500 ${
            this.open ? 'rotate-90' : ''
          } ${expandable ? '' : 'invisible'}"
          >▶</span
        >
        <span
          role="img"
          class="size-3.5 self-center ${swatchClass}"
          aria-label=${swatchLabel}
          title=${swatchLabel}
        ></span>
        <span class="min-w-0">
          ${
            this.side === 'baml'
              ? bamlSignature(this.symbol as BamlSymbol, this.matrix)
              : tsSignature(this.symbol as TsSymbol, this.matrix)
          }${
            this.node.children.length > 0
              ? html`<span class="ml-2 text-[0.7rem] text-zinc-500"
                >${this.node.children.length} members</span
              >`
              : nothing
          }
        </span>
      </div>
      ${this.open ? this.body() : nothing}
      ${
        this.open && this.node.children.length > 0
          ? html`<div>
            ${this.node.children.map(
              (child) => html`
                <matrix-symbol
                  class="block border-t border-zinc-200 dark:border-zinc-800"
                  .side=${this.side}
                  .node=${child}
                  .links=${this.links}
                  .matrix=${this.matrix}
                  .target=${this.targetFor(child)}
                  .depth=${this.depth + 1}
                ></matrix-symbol>
              `,
            )}
          </div>`
          : nothing
      }
    `;
  }

  /** The judgements recording that nothing on the other side does this
   *  symbol's job. Each carries the reasoning that makes it worth reading. */
  private get absences(): Judgement[] {
    return this.own.filter((judgement) => judgement.verdict === 'none');
  }

  // What the run concluded, and why. A pairing without its reasoning is a bare
  // assertion, and an absence without one is indistinguishable from silence —
  // the reasoning is the thing this report is for.
  private body(): TemplateResult | typeof nothing {
    const doc = this.symbol.doc;
    const facts = this.facts;
    if (!doc && facts.length === 0 && this.own.length === 0) return nothing;
    const other = this.side === 'baml' ? 'TypeScript' : 'BAML';
    return html`
      <div class="pt-0.5 pr-3 pb-3 pl-10">
        ${doc ? html`<p class="my-0.5 text-sm whitespace-pre-wrap">${doc}</p>` : nothing}
        ${
          facts.length > 0
            ? html`<p class="my-1 font-mono text-xs text-zinc-500">${facts}</p>`
            : nothing
        }
        ${
          this.counterparts.length > 0
            ? html`
              ${SECTION(other)}
              ${this.counterparts.map((link) => this.counterpart(link))}
            `
            : nothing
        }
        ${this.absences.map((judgement) => this.absence(judgement, other))}
      </div>
    `;
  }

  /**
   * A recorded absence: the conclusion that nothing on the other side does this
   * symbol's job, with the reasoning that reached it.
   *
   * Worth a block of its own rather than a line, because most of them came from
   * a judge refuting a proposal — and what it refused is as useful as the
   * conclusion. `baml.Float.parse` has no counterpart *because* `parseFloat`
   * returns NaN where BAML throws, and the reader wants to see the near miss.
   */
  private absence(judgement: Judgement, other: string): TemplateResult {
    return html`
      ${SECTION(`no ${other} counterpart`)}
      <p class="my-1 text-sm text-zinc-600 dark:text-zinc-400">
        ${judgement.reason ?? 'no reason recorded'}
      </p>
      ${
        judgement.rejected.length > 0
          ? html`<div class="my-1 text-xs text-zinc-500">
            considered and refused:
            ${judgement.rejected.map(
              (id, index) =>
                html`${index > 0 ? ', ' : ''}<span class="font-mono">${id}</span>`,
            )}
          </div>`
          : nothing
      }
      ${this.provenance(judgement)}
    `;
  }

  /**
   * How much weight a judgement carries: what established it, how sure it was,
   * and whether anything checked it.
   *
   * Shown for every judgement rather than only model ones. A name match that no
   * judge has examined and a pairing an adversarial judge upheld are different
   * claims, and they used to render identically.
   */
  private provenance(judgement: Judgement): TemplateResult {
    const parts: string[] = [BASIS[judgement.basis] ?? judgement.basis];
    if (judgement.confidence) parts.push(`${judgement.confidence} confidence`);
    parts.push(judgement.verified ? 'checked by a second pass' : 'unchecked');
    return html`<p class="my-1 text-[0.68rem] text-zinc-500">${parts.join(' · ')}</p>`;
  }

  private counterpart(link: Judgement): TemplateResult | typeof nothing {
    const otherSide: Side = this.side === 'baml' ? 'ts' : 'baml';
    // A judgement names its ends by id; the views address rows by index.
    const wanted = this.side === 'baml' ? link.ts : link.baml;
    const symbols: Array<BamlSymbol | TsSymbol> =
      otherSide === 'ts' ? this.matrix.ts : this.matrix.baml;
    const index = symbols.findIndex((candidate) => candidate.id === wanted);
    const symbol = symbols[index];
    if (!symbol) return nothing;
    return html`
      <div class="my-1">
        <div class="font-mono text-[0.8rem]">
          ${this.crossLink(otherSide, index, symbol.display)}
        </div>
        <div class="font-mono text-xs text-zinc-500">
          ${this.signatureOf(symbol, otherSide)}
        </div>
        ${
          link.verdict === 'divergent'
            ? html`<p
              class="my-1 border-l-2 border-amber-400/70 pl-2 text-sm
                     text-zinc-700 dark:border-amber-500/60 dark:text-zinc-300"
            >
              ${link.divergence ?? 'the two differ, but how was not recorded'}
            </p>`
            : nothing
        }
        ${
          link.reason
            ? html`<p class="my-1 text-xs text-zinc-500">${link.reason}</p>`
            : nothing
        }
        ${this.provenance(link)}
      </div>
    `;
  }

  /**
   * A counterpart, as a way into the other view.
   *
   * The destination is known by index here, so it needs no name to be found —
   * only to be written into the URL, which some symbols (an impl method, whose
   * path names its receiver rather than a namespace) cannot supply. Those still
   * navigate; they just do not leave an address behind.
   */
  private crossLink(
    side: Side,
    index: number,
    label: string,
  ): TemplateResult | string {
    const symbols = SymbolIndex.for(this.matrix);
    const ref = symbols.place(side, index);
    if (!ref) return label;
    const address = symbols.addressOf(ref);
    return html`<a
      class="cursor-pointer underline-offset-2 hover:underline"
      href=${address === null ? nothing : `#${side}/${encodeURIComponent(address)}`}
      title=${`Go to this symbol in the ${side === 'baml' ? 'BAML' : 'TypeScript'} view`}
      @click=${(event: MouseEvent) => {
        event.preventDefault();
        dispatchGoto(event.currentTarget as Element, ref, address);
      }}
      >${label}</a
    >`;
  }
}

customElements.define('matrix-symbol', MatrixSymbolElement);
