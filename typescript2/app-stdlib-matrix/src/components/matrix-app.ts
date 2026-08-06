import { html, LitElement, nothing, type TemplateResult } from 'lit';
import {
  GOTO_EVENT,
  type GotoDetail,
  type GotoTarget,
  type Ref,
  SymbolIndex,
} from '../navigation';
import { Links, type Side, type SymbolMatrix, type TreeNode } from '../types';
import './matrix-group';

// The page: load a report, then show it from either language's point of view.
//
// The two stdlibs are not 1:1, so no single hierarchy serves both readers. The
// report stores each side flat with judgements as a relation, and each
// view groups its own side — BAML by owner, TypeScript by container — reading
// the same links. Switching views re-groups; it does not reload or refilter.
//
// The report is fetched rather than bundled, so one build renders any run's
// artifact. `?src=` points at one; the default is a sibling `matrix.json`.
//
// Where the reader is looking is part of the address: `?view=` for the side and
// `#<side>/<name>` for a symbol within it. Following a reference writes that
// fragment, so the back button retraces the path and a link can be shared.

const LEGEND = [
  ['swatch-both', 'in both'],
  ['swatch-baml', 'BAML only'],
  ['swatch-ts', 'TypeScript only'],
] as const;

export class MatrixAppElement extends LitElement {
  static properties = {
    error: { state: true },
    matrix: { state: true },
    side: { state: true },
    target: { state: true },
  };

  declare matrix: SymbolMatrix | null;
  declare side: Side;
  declare error: string | null;
  declare target: GotoTarget | null;
  #links: Links | null = null;
  #requests = 0;
  #modifier = false;

  constructor() {
    super();
    this.matrix = null;
    this.side = 'baml';
    this.error = null;
    this.target = null;
  }

  protected createRenderRoot() {
    return this;
  }

  connectedCallback() {
    super.connectedCallback();
    this.className = 'mx-auto block max-w-5xl px-4 pt-6 pb-16';
    const params = new URLSearchParams(location.search);
    const view = params.get('view');
    if (view === 'ts' || view === 'baml') this.side = view;
    this.addEventListener(GOTO_EVENT, this.#onGoto as EventListener);
    addEventListener('popstate', this.#onPopState);
    // Type references only offer themselves while the modifier is held, which
    // is also the only time clicking one does anything. Tracking it on the
    // document keeps that in CSS: pressing a key must not re-render the page.
    for (const event of ['keydown', 'keyup', 'mousemove'] as const) {
      addEventListener(event, this.#onModifier);
    }
    addEventListener('blur', this.#clearModifier);
    void this.#load(params.get('src') ?? './matrix.json');
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    removeEventListener('popstate', this.#onPopState);
    for (const event of ['keydown', 'keyup', 'mousemove'] as const) {
      removeEventListener(event, this.#onModifier);
    }
    removeEventListener('blur', this.#clearModifier);
    this.#clearModifier();
  }

  #onModifier = (event: Event) => {
    const held =
      (event as KeyboardEvent | MouseEvent).metaKey === true ||
      (event as KeyboardEvent | MouseEvent).ctrlKey === true;
    if (held === this.#modifier) return;
    this.#modifier = held;
    document.documentElement.classList.toggle('mod-held', held);
  };

  #clearModifier = () => {
    this.#modifier = false;
    document.documentElement.classList.remove('mod-held');
  };

  #onGoto = (event: CustomEvent<GotoDetail>) => {
    const { ref, name } = event.detail;
    if (name !== null) {
      history.pushState(null, '', `#${ref.side}/${encodeURIComponent(name)}`);
    }
    this.#focus(ref);
  };

  #onPopState = () => {
    this.#followHash();
  };

  /** Opens the page onto a place, switching sides if it lives in the other. */
  #focus(ref: Ref) {
    this.#select(ref.side);
    this.#requests += 1;
    this.target = { ...ref, nonce: this.#requests };
  }

  /** `#<side>/<name>`, when the report knows that name. */
  #followHash() {
    const raw = location.hash.slice(1);
    const index = this.matrix ? SymbolIndex.for(this.matrix) : null;
    if (raw.length === 0 || !index) return;
    const cut = raw.indexOf('/');
    const side = raw.slice(0, cut);
    if (side !== 'baml' && side !== 'ts') return;
    const ref = index.resolve(side, decodeURIComponent(raw.slice(cut + 1)));
    if (ref) this.#focus(ref);
  }

  async #load(source: string) {
    try {
      const response = await fetch(source);
      if (!response.ok)
        throw new Error(`${response.status} ${response.statusText}`);
      const matrix = (await response.json()) as SymbolMatrix;
      this.#links = new Links(matrix);
      this.matrix = matrix;
      this.#followHash();
    } catch (cause) {
      const reason = cause instanceof Error ? cause.message : String(cause);
      this.error =
        `Could not load ${source} — ${reason}. Generate a report with ` +
        'tools/stdlib-matrix/run, or point ?src= at one.';
    }
  }

  #select(side: Side) {
    if (this.side === side) return;
    this.side = side;
    const url = new URL(location.href);
    url.searchParams.set('view', side);
    history.replaceState(null, '', url);
  }

  /** Symbols of the viewed side, in their own language's hierarchy. */
  get #groups(): Array<[string, TreeNode[]]> {
    const matrix = this.matrix;
    if (!matrix) return [];
    return SymbolIndex.for(matrix).groups(this.side);
  }

  render() {
    return html`
      <h1 class="mb-1 text-xl font-semibold">BAML ↔ TypeScript stdlib matrix</h1>
      ${this.#provenance()} ${this.#tabs()}
      <div class="mb-5 flex flex-wrap items-center gap-4 text-xs text-zinc-500">
        ${LEGEND.map(
          ([cls, label]) =>
            html`<span class="flex items-center gap-1.5"
              ><i class="size-3 ${cls}"></i>${label}</span
            >`,
        )}
      </div>
      ${this.#counts()} ${this.#body()}
    `;
  }

  #provenance() {
    const p = this.matrix?.provenance;
    return html`<p class="mb-4 font-mono text-xs text-zinc-500">
      ${
        p
          ? [
              `BAML surface ${p.baml_surface_sha256.slice(0, 12)}…`,
              `TypeScript ${p.typescript_version}`,
              `@types/node ${p.types_node_version ?? 'absent'}`,
            ].join(' · ')
          : ''
      }
    </p>`;
  }

  #tabs() {
    return html`<div class="mb-4 flex gap-1">
      ${(
        [
          ['baml', 'BAML view'],
          ['ts', 'TypeScript view'],
        ] as const
      ).map(([side, label]) => {
        const active = this.side === side;
        return html`<button
          type="button"
          aria-pressed=${active}
          class="rounded-md px-3 py-1 text-sm focus-visible:outline-2
                 focus-visible:outline-blue-500 ${
                   active
                     ? 'bg-zinc-900 text-white dark:bg-zinc-100 dark:text-zinc-900'
                     : 'text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800'
                 }"
          @click=${() => {
            // The other side's tree has no place for the current destination.
            this.target = null;
            this.#select(side);
          }}
        >
          ${label}
        </button>`;
      })}
    </div>`;
  }

  #counts(): TemplateResult | typeof nothing {
    const matrix = this.matrix;
    if (!matrix) return nothing;
    const c = matrix.counts;
    const parts: Array<[number, string]> =
      this.side === 'baml'
        ? [
            [c.matched, 'with a TypeScript counterpart'],
            [c.unmatched, 'judged to have none'],
            [c.unjudged, 'unjudged'],
          ]
        : [
            [c.ts_symbols - c.ts_unclaimed, 'with a BAML counterpart'],
            [c.ts_unclaimed, 'without'],
          ];
    const total = this.side === 'baml' ? c.baml_symbols : c.ts_symbols;
    return html`<p class="mb-4 text-sm">
      ${parts.map(
        ([value, label], index) =>
          html`<b class="font-semibold">${value}</b> ${label}${
            index < parts.length - 1 ? ' · ' : ''
          }`,
      )}
      of ${total} symbols
    </p>`;
  }

  #body() {
    if (this.error)
      return html`<p class="py-8 text-zinc-500">${this.error}</p>`;
    if (!this.matrix || !this.#links)
      return html`<p class="py-8 text-zinc-500">Loading…</p>`;
    const links = this.#links;
    const matrix = this.matrix;
    return html`<div>
      ${this.#groups.map(
        ([name, members]) => html`
          <matrix-group
            class="mb-1.5 block overflow-hidden rounded-lg border border-zinc-200 dark:border-zinc-800"
            .name=${name}
            .side=${this.side}
            .members=${members}
            .links=${links}
            .matrix=${matrix}
            .target=${this.target?.group === name ? this.target : null}
          ></matrix-group>
        `,
      )}
    </div>`;
  }
}

customElements.define('matrix-app', MatrixAppElement);
