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

const SWATCH = {
  baml: ['swatch-baml', 'BAML only'],
  both: ['swatch-both', 'present in both'],
  ts: ['swatch-ts', 'TypeScript only'],
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
    if (this.counterparts.length > 0) return SWATCH.both;
    return this.side === 'baml' ? SWATCH.baml : SWATCH.ts;
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

  // Absence is already shown by the swatch, so the body says only what it has:
  // no "no counterpart" line, and nothing at all when there is nothing to say.
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
              <h4
                class="mt-3 mb-1 text-[0.68rem] font-semibold tracking-wider
                       text-zinc-500 uppercase"
              >
                ${other}
              </h4>
              ${this.counterparts.map((link) => this.counterpart(link))}
            `
            : nothing
        }
      </div>
    `;
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
          link.basis === 'model'
            ? html`<div class="text-xs text-zinc-500">
              ${link.confidence ?? '?'} confidence — ${link.reason ?? ''}
            </div>`
            : nothing
        }
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
