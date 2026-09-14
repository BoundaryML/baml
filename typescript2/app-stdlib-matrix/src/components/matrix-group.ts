import { html, LitElement, nothing, type PropertyValues } from 'lit';
import { compositionBar } from '../composition';
import { flash, type GotoTarget } from '../navigation';
import type { Links, Side, SymbolMatrix, TreeNode } from '../types';
import './matrix-symbol';

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
    rowScale: { type: Number },
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
  /** The largest group's size, which every group bar is drawn as a fraction
   *  of. */
  declare scale: number;
  /** The largest member-holding row's size, which the rows inside are drawn as
   *  a fraction of. Groups and rows are scaled against their own peers. */
  declare rowScale: number;
  #focused = -1;

  constructor() {
    super();
    this.name = '';
    this.side = 'baml';
    this.members = [];
    this.target = null;
    this.open = false;
    this.scale = 0;
    this.rowScale = 0;
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

  render() {
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
        <span class="ml-auto">
          ${compositionBar(this.members, this.links, this.side, this.scale, 'group')}
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
                  .scale=${this.rowScale}
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
