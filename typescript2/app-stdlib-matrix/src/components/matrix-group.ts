import {
  html,
  LitElement,
  nothing,
  type PropertyValues,
  type TemplateResult,
} from 'lit';
import { flash, type GotoTarget } from '../navigation';
import {
  countNodes,
  type Links,
  type Side,
  type SymbolMatrix,
  type SymbolState,
  stateOf,
  type TreeNode,
} from '../types';
import './matrix-symbol';

/**
 * The bar's segments, in reading order: answered, answered differently,
 * answered with a "no", and unanswered.
 *
 * The same visual vocabulary as the row swatches, so the legend explains both,
 * but flat classes rather than the `swatch-*` utilities — those carry a border
 * and a radius that an 8px slice cannot afford.
 */
function segmentsFor(
  side: Side,
): ReadonlyArray<readonly [SymbolState, string, string]> {
  // Solid is the *other* language, matching the swatches: from the TypeScript
  // view an "in both" segment is purple, because what it reports is presence in
  // BAML.
  const shared = [
    ['match', side === 'ts' ? 'seg-baml' : 'seg-ts', 'in both'],
    ['divergent', 'seg-striped', 'in both, reached differently'],
  ] as const;
  // The same states the legend names, in the same colours. The BAML side has no
  // judged-absence state, so its solid segment is the unnamed one.
  return side === 'ts'
    ? [
        ...shared,
        ['unnecessary', 'seg-unnecessary', 'unnecessary in BAML'],
        ['none', 'seg-ts', 'no BAML counterpart'],
        ['unjudged', 'seg-unjudged', 'not yet judged'],
      ]
    : [
        ...shared,
        ['unnecessary', 'seg-unnecessary', 'unnecessary in BAML'],
        ['unjudged', 'seg-baml', 'no judgement names it'],
      ];
}

// One group in the current view: a BAML owner (namespace, type, or impl) or a
// TypeScript container.
//
// Rows render only while open. A few thousand rows built eagerly is a visible
// pause for content most readers never open, and Lit's template caching makes
// re-opening cheap.

export class MatrixGroupElement extends LitElement {
  static properties = {
    links: { attribute: false },
    matrix: { attribute: false },
    members: { attribute: false },
    name: {},
    open: { state: true, type: Boolean },
    scale: { type: Number },
    side: {},
    target: { attribute: false },
  };

  declare name: string;
  declare side: Side;
  declare members: TreeNode[];
  declare links: Links;
  declare matrix: SymbolMatrix;
  declare target: GotoTarget | null;
  declare open: boolean;
  /** The largest group's size, which every bar is drawn as a fraction of. */
  declare scale: number;
  #focused = -1;

  constructor() {
    super();
    this.name = '';
    this.side = 'baml';
    this.members = [];
    this.target = null;
    this.open = false;
    this.scale = 0;
  }

  protected createRenderRoot() {
    return this;
  }

  // Only the group a request leads into is given the target, so being handed
  // one is reason enough to open: the rows inside cannot be reached otherwise.
  // Opening follows the arrival of a request rather than its presence, so a
  // group the reader closes afterwards stays closed.
  protected willUpdate(changed: PropertyValues) {
    if (changed.has('target') && this.target) this.open = true;
  }

  protected updated() {
    const target = this.target;
    if (!target || target.path.length > 0 || target.nonce === this.#focused)
      return;
    // The destination is the group itself: a TypeScript container, which has no
    // row of its own.
    this.#focused = target.nonce;
    this.scrollIntoView({ behavior: 'smooth', block: 'start' });
    flash(this);
  }

  /**
   * How this group's symbols came out, counting every descendant.
   *
   * Members nest under their container, so counting only the top level would
   * describe a module by its type names rather than by its surface — `node:os`
   * would read as 20 and `(globals)` as 57, when the second holds twenty times
   * the API.
   */
  private get tally(): Record<SymbolState, number> {
    const counts: Record<SymbolState, number> = {
      divergent: 0,
      match: 0,
      none: 0,
      unjudged: 0,
      unnecessary: 0,
    };
    const walk = (nodes: TreeNode[]) => {
      for (const node of nodes) {
        counts[stateOf(this.links, this.side, node.index)] += 1;
        walk(node.children);
      }
    };
    walk(this.members);
    return counts;
  }

  /**
   * The group's composition, as a bar whose length is its share of the largest
   * group's.
   *
   * Numbers made the groups look alike: `(globals)` holds 1118 symbols and
   * `node:querystring` holds 4, and both rendered as two short counts. Length
   * carries the scale and the segments carry the breakdown, so a reader can see
   * at a glance both how much surface a module has and how much of it is
   * answered.
   */
  private bar(total: number): TemplateResult {
    const counts = this.tally;
    const share = this.scale > 0 ? (total / this.scale) * 100 : 0;
    return html`<span
      class="flex h-2 overflow-hidden rounded-full"
      style=${`width: ${Math.max(share, 1.5)}%`}
      >${segmentsFor(this.side).map(([state, cls, label]) =>
        counts[state] === 0
          ? nothing
          : html`<span
              class=${cls}
              style=${`flex: ${counts[state]}`}
              title=${`${counts[state]} ${label}`}
            ></span>`,
      )}</span
    >`;
  }

  render() {
    const total = countNodes(this.members);
    return html`
      <button
        type="button"
        class="flex w-full items-baseline gap-3 bg-zinc-50 px-3 py-2.5 text-left
               hover:bg-zinc-100 focus-visible:outline-2 focus-visible:-outline-offset-2
               focus-visible:outline-blue-500 dark:bg-zinc-900 dark:hover:bg-zinc-800"
        aria-expanded=${this.open}
        @click=${() => {
          this.open = !this.open;
        }}
      >
        <span
          aria-hidden="true"
          class="text-[0.7rem] text-zinc-500 ${this.open ? 'rotate-90' : ''}"
          >▶</span
        >
        <span class="font-mono font-semibold">${this.name}</span>
        <span class="ml-auto flex shrink-0 items-center gap-2">
          <span class="w-10 text-right text-xs tabular-nums text-zinc-500">${total}</span>
          <span class="flex w-40 self-center">${this.bar(total)}</span>
        </span>
      </button>
      ${
        this.open
          ? html`<div>
            ${this.members.map(
              (node) => html`
                <matrix-symbol
                  class="block border-t border-zinc-200 first:border-t-0 dark:border-zinc-800"
                  .side=${this.side}
                  .node=${node}
                  .links=${this.links}
                  .matrix=${this.matrix}
                  .target=${this.target?.path[0] === node.index ? this.target : null}
                  .depth=${0}
                ></matrix-symbol>
              `,
            )}
          </div>`
          : nothing
      }
    `;
  }
}

customElements.define('matrix-group', MatrixGroupElement);
