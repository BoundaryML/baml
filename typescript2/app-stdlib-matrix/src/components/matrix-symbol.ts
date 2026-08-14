import {
  html,
  LitElement,
  nothing,
  type PropertyValues,
  type TemplateResult,
} from 'lit';
import { compositionBar } from '../composition';
import {
  dispatchGoto,
  flash,
  type GotoTarget,
  SymbolIndex,
} from '../navigation';
import { bamlSignature, tsSignature } from '../signature';
import {
  type BamlSymbol,
  type Judgement,
  type Links,
  type Side,
  type SymbolMatrix,
  type SymbolState,
  stateOf,
  type TreeNode,
  type TsSymbol,
} from '../types';

// One symbol row, seen from whichever side the reader is viewing.
//
// The swatch answers "what does the other language do about this" — see
// `swatchFor`.
//
// A row is only handed a `target` when it is on the path to one — its parent
// passes it down to the one child that leads there — so holding it at all means
// "open, and if the path ends here, this is the destination".

/** A heading over one part of an opened row. */
const SECTION = (label: string) => html`<h4
  class="mt-3 mb-1 text-[0.68rem] font-semibold tracking-wider text-zinc-500 uppercase"
>
  ${label}
</h4>`;

/** What a judgement rests on, said in words rather than in the report's slug. */
const BASIS: Record<string, string> = {
  model: 'judged by a model',
  'no-candidates': 'nothing comparable exists to consider',
};

/**
 * The square, chosen by what was concluded and by where the reader stands.
 *
 * A solid square is one language, and it is always the *other* one when the
 * symbol exists in both: from the TypeScript view, purple says "this is in BAML
 * too" and yellow says "this is TypeScript's alone". The viewing side's own
 * colour therefore always means "only here", which is the reading a scan wants.
 *
 * Stripes are both languages interleaved: the same operation, reached
 * differently. Red belongs to neither surface — it says BAML answers the
 * question in the language, so the API does not arise.
 */
function swatchFor(state: SymbolState, side: Side): readonly [string, string] {
  const own = side === 'ts' ? 'swatch-ts' : 'swatch-baml';
  const other = side === 'ts' ? 'swatch-baml' : 'swatch-ts';
  switch (state) {
    case 'match':
      return [other, 'present in both'];
    case 'divergent':
      return ['swatch-striped', 'present in both, reached differently'];
    case 'unnecessary':
      return ['swatch-unnecessary', 'unnecessary in BAML'];
    case 'none':
      return [own, 'no BAML counterpart'];
    default:
      // An absence names no BAML symbol, so it never indexes from that end: a
      // BAML symbol is either named by a judgement or not, and "not" is silence
      // rather than a finding. Only the TypeScript side can tell "examined,
      // nothing does this" from "nothing has looked yet", so only it gets the
      // hollow square; on the BAML side the same state is solid, and means no
      // judgement happens to name this symbol.
      return side === 'ts'
        ? ['swatch-unjudged-ts', 'not yet judged']
        : [own, 'no judgement names it'];
  }
}

export class MatrixSymbolElement extends LitElement {
  static properties = {
    depth: { type: Number },
    links: { attribute: false },
    matrix: { attribute: false },
    node: { attribute: false },
    open: { state: true, type: Boolean },
    scale: { type: Number },
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
  /** The largest member-holding row's size, which this row's bar is drawn as a
   *  fraction of. Rows are scaled against rows, never against the groups. */
  declare scale: number;
  /** The last request this row scrolled for, so a repeat request scrolls again
   *  but a re-render for any other reason does not. */
  #focused = -1;

  constructor() {
    super();
    this.side = 'baml';
    this.target = null;
    this.depth = 0;
    this.open = false;
    this.scale = 0;
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
    return swatchFor(
      stateOf(this.links, this.side, this.node.index),
      this.side,
    );
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
        <span class="flex min-w-0 items-baseline gap-3">
          <span class="min-w-0 flex-1">
            ${
              this.side === 'baml'
                ? bamlSignature(this.symbol as BamlSymbol, this.matrix)
                : tsSignature(this.symbol as TsSymbol, this.matrix)
            }
          </span>
          ${
            // A row that holds members gets the same bar its group does, so a
            // class and the module around it are read the same way.
            this.node.children.length > 0
              ? compositionBar(
                  this.node.children,
                  this.links,
                  this.side,
                  this.scale,
                  'row',
                )
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
                  .scale=${this.scale}
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

  /** The judgements recording that BAML makes the question not arise. */
  private get unnecessary(): Judgement[] {
    return this.own.filter((judgement) => judgement.verdict === 'unnecessary');
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
              ${this.counterparts.map((link) => this.judgement(link))}
            `
            : nothing
        }
        ${
          this.unnecessary.length > 0
            ? html`
              ${SECTION('unnecessary in BAML')}
              ${this.unnecessary.map((link) => this.judgement(link))}
            `
            : nothing
        }
        ${this.absences.map((judgement) => this.absence(judgement, other))}
      </div>
    `;
  }

  /**
   * The same job written in each language, side by side.
   *
   * Shown wherever a judgement carries one, not only for the unnecessary ones:
   * a divergence is exactly the case where prose is weakest and two lines of
   * code are clearest.
   */
  private example(judgement: Judgement): TemplateResult | typeof nothing {
    const example = judgement.example;
    if (!example) return nothing;
    const pane = (label: string, code: string) => html`<div class="min-w-0">
      <div class="mb-0.5 text-[0.62rem] tracking-wider text-zinc-500 uppercase">${label}</div>
      <pre
        class="overflow-x-auto rounded bg-zinc-100 px-2 py-1.5 font-mono
               text-[0.75rem] whitespace-pre dark:bg-zinc-900"
      ><code>${code}</code></pre>
    </div>`;
    return html`
      <div class="my-1.5 grid gap-2 sm:grid-cols-2">
        ${pane('TypeScript', example.typescript)} ${pane('BAML', example.baml)}
      </div>
      ${
        example.note
          ? html`<p class="my-1 text-xs text-zinc-500">${example.note}</p>`
          : nothing
      }
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
      ${this.example(judgement)} ${this.provenance(judgement)}
    `;
  }

  /**
   * How much weight a judgement carries: what established it, how sure it was,
   * and whether anything checked it.
   *
   * Shown for every judgement, because a proposal no judge has examined and a
   * pairing an adversarial judge upheld are different claims, and they used to
   * render identically.
   */
  private provenance(judgement: Judgement): TemplateResult {
    const parts: string[] = [BASIS[judgement.basis] ?? judgement.basis];
    if (judgement.confidence) parts.push(`${judgement.confidence} confidence`);
    parts.push(judgement.verified ? 'checked by a second pass' : 'unchecked');
    return html`<p class="my-1 text-[0.68rem] text-zinc-500">${parts.join(' · ')}</p>`;
  }

  /**
   * The ids on the far side of a judgement, from wherever the reader is.
   *
   * Plural in one direction and singular in the other, because the relation is:
   * a judgement answers for one TypeScript symbol and may name several BAML
   * ones — `child_process.spawn` is `baml.sys.exec` together with the
   * `ShellOutput` it returns.
   */
  private facing(link: Judgement): string[] {
    return this.side === 'baml' ? [link.ts] : link.baml;
  }

  /**
   * One judgement: everything it points at, then why — each said once.
   *
   * Structured around the judgement rather than around its endpoints, because
   * the reasoning belongs to the judgement. Rendering per endpoint repeated the
   * reason, the example and the provenance once per symbol named, which for the
   * 173 judgements naming more than one read as several separate findings that
   * happened to agree.
   */
  private judgement(link: Judgement): TemplateResult {
    const otherSide: Side = this.side === 'baml' ? 'ts' : 'baml';
    const symbols: Array<BamlSymbol | TsSymbol> =
      otherSide === 'ts' ? this.matrix.ts : this.matrix.baml;
    const found = this.facing(link)
      .map((wanted) =>
        symbols.findIndex((candidate) => candidate.id === wanted),
      )
      .filter((index) => index >= 0);
    return html`
      <div class="my-1">
        ${found.map((index) => {
          const symbol = symbols[index];
          if (!symbol) return nothing;
          return html`<div class="mb-1">
            <div class="font-mono text-[0.8rem]">
              ${this.crossLink(otherSide, index, symbol.display)}
            </div>
            <div class="font-mono text-xs text-zinc-500">
              ${this.signatureOf(symbol, otherSide)}
            </div>
          </div>`;
        })}
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
            ? html`<p class="my-1 text-sm text-zinc-600 dark:text-zinc-400">${link.reason}</p>`
            : nothing
        }
        ${this.example(link)} ${this.provenance(link)}
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
