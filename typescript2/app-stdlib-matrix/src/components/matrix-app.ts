import { html, LitElement, nothing, type TemplateResult } from 'lit';
import { rowScaleOf } from '../composition';
import {
  GOTO_EVENT,
  type GotoDetail,
  type GotoTarget,
  type Ref,
  SymbolIndex,
} from '../navigation';
import {
  countNodes,
  Links,
  type Side,
  type SymbolMatrix,
  type TreeNode,
} from '../types';
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

/**
 * What the squares mean, for the side being viewed.
 *
 * Side-aware because the states are: an absence is recorded against the
 * TypeScript symbol and names no BAML one, so only that side can distinguish
 * "examined and found to have none" from "nothing has looked yet". Naming both
 * languages in one fixed list would describe squares that are not on screen.
 */
function legendFor(side: Side): ReadonlyArray<readonly [string, string]> {
  // Solid is the *other* language, so the viewing side's own colour always
  // means "only here" — see `swatchFor` in matrix-symbol.
  const shared = [
    [side === 'ts' ? 'swatch-baml' : 'swatch-ts', 'in both'],
    ['swatch-striped', 'in both, reached differently'],
  ] as const;
  return side === 'ts'
    ? [
        ...shared,
        ['swatch-ts', 'no BAML counterpart'],
        ['swatch-unnecessary', 'unnecessary in BAML'],
        ['swatch-unjudged-ts', 'not yet judged'],
      ]
    : // The BAML side has neither of the last two. An absence names no BAML
      // symbol, and a symbol named by an `unnecessary` judgement is named as
      // how you would do the thing anyway, so it reads as claimed — see
      // `stateOf`. Here a symbol is either named by a judgement or not.
      [...shared, ['swatch-baml', 'no judgement names it']];
}

/**
 * The report shape this build understands. Bumped by the producer whenever the
 * relation changes — v2 moved the judgement key to the TypeScript side.
 */
const REPORT_FORMAT = 2;

/**
 * Which report to load, given whatever `?src=` said.
 *
 * A relative path only, resolved against this page. The parameter exists so one
 * build can render any run's artifact — a report beside the page, a second one
 * for comparison — not so a link can point the page at another origin. A public
 * URL that renders attacker-hosted JSON as this site's content is a different
 * feature, and not one worth having.
 */
export function reportSource(requested: string | null): string {
  const fallback = './matrix.json';
  if (requested === null || requested.length === 0) return fallback;
  let resolved: URL;
  try {
    // Anything the URL parser accepts as absolute — including a scheme-relative
    // `//host/x` — names an origin, and so is refused.
    resolved = new URL(requested, location.href);
  } catch {
    // The parser rejects some inputs outright (`http://[`). Throwing here would
    // escape `connectedCallback` and leave the page on "Loading…" forever,
    // since the load's own error handling is downstream of this call.
    return fallback;
  }
  if (resolved.origin !== location.origin) return fallback;
  // The resolved URL, not a path rebuilt from its parts. Returning
  // `pathname + search` passed the origin check and then threw the origin
  // away: a `..` that pops past the root leaves a pathname beginning with
  // `//`, and `fetch("//host/x")` is scheme-relative, not a path. So
  // `?src=/..//evil.example/matrix.json` cleared the check and then fetched
  // evil.example — the precise thing this function exists to prevent.
  return resolved.href;
}

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
    void this.#load(reportSource(params.get('src')));
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    removeEventListener('popstate', this.#onPopState);
    for (const event of ['keydown', 'keyup', 'mousemove'] as const) {
      removeEventListener(event, this.#onModifier);
    }
    removeEventListener('blur', this.#clearModifier);
    this.removeEventListener(GOTO_EVENT, this.#onGoto as EventListener);
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

  /**
   * `#<side>/<name>`, when the report knows that name.
   *
   * Everything here is best-effort: the fragment is whatever was in the address
   * bar, and a bad one must not cost the reader the report. It used to: this
   * runs inside `#load`'s try, *after* the matrix is assigned, so a fragment
   * like `#ts/%zz` made `decodeURIComponent` throw and the page reported
   * "Could not load ./matrix.json — URI malformed" over a report that had
   * loaded perfectly. Via `popstate` the same throw was uncaught entirely.
   */
  #followHash() {
    const raw = location.hash.slice(1);
    const index = this.matrix ? SymbolIndex.for(this.matrix) : null;
    if (raw.length === 0 || !index) return;
    const cut = raw.indexOf('/');
    // No separator means no name. `slice(0, -1)` would drop the last character
    // and read `#tsX` as side `ts`, which is a different symbol's address.
    if (cut < 0) return;
    const side = raw.slice(0, cut);
    if (side !== 'baml' && side !== 'ts') return;
    let name: string;
    try {
      name = decodeURIComponent(raw.slice(cut + 1));
    } catch {
      return;
    }
    const ref = index.resolve(side, name);
    if (ref) this.#focus(ref);
  }

  async #load(source: string) {
    try {
      const response = await fetch(source);
      if (!response.ok)
        throw new Error(`${response.status} ${response.statusText}`);
      const matrix = (await response.json()) as SymbolMatrix;
      // The producer refuses a report it cannot read, and so does the workflow;
      // this is the third consumer and the only public one. Rendering a shape
      // this build does not understand is worse than saying so — the fields it
      // still recognises would draw a plausible page over the wrong data.
      if (matrix.format_version !== REPORT_FORMAT) {
        this.error =
          `${source} is report format v${matrix.format_version}; this page reads ` +
          `v${REPORT_FORMAT}. Rebuild it with tools/stdlib-matrix/run.`;
        return;
      }
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
        ${legendFor(this.side).map(
          ([cls, label]) =>
            html`<span class="flex items-center gap-1.5"
              ><i class="size-3 ${cls}"></i>${label}</span
            >`,
        )}
      </div>
      ${this.#counts()} ${this.#failures()} ${this.#body()}
    `;
  }

  /**
   * Calls that did not come back.
   *
   * Rendered here rather than left to the report, because a run that half
   * failed produces a report that reads exactly like a complete one: the
   * symbols it never got to are simply unjudged, which is also what a symbol
   * nobody has reached yet looks like. This is the only place the difference
   * is visible.
   */
  #failures(): TemplateResult | typeof nothing {
    const failures = this.matrix?.failures ?? [];
    if (failures.length === 0) return nothing;
    return html`<div
      class="mb-5 rounded-lg border border-amber-400/60 bg-amber-50 px-3 py-2
             text-sm dark:border-amber-500/40 dark:bg-amber-950/30"
    >
      <p class="font-semibold">
        ${failures.length} ${failures.length === 1 ? 'call' : 'calls'} did not come back
      </p>
      <p class="mt-0.5 text-xs text-zinc-600 dark:text-zinc-400">
        Whatever those calls would have judged is unjudged below, which is
        indistinguishable from not having been reached.
      </p>
      <ul class="mt-1.5 space-y-0.5 font-mono text-xs text-zinc-600 dark:text-zinc-400">
        ${failures.map(
          (failure) =>
            html`<li>${failure.pass} · ${failure.subject} — ${failure.reason}</li>`,
        )}
      </ul>
    </div>`;
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
      this.side === 'ts'
        ? [
            [c.matched, 'with a BAML counterpart'],
            [c.unnecessary, 'unnecessary in BAML'],
            [c.unmatched, 'judged to have none'],
            [c.unjudged, 'unjudged'],
          ]
        : // Not "judged to have none" and "unjudged": an absence names no BAML
          // symbol, so from this side there is only named and not-named.
          [
            [c.baml_symbols - c.baml_unclaimed, 'named by a judgement'],
            [c.baml_unclaimed, 'not named by any'],
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
    const groups = this.#groups;
    // Every bar is a fraction of the largest group's size, so the widths are
    // comparable across the page rather than each filling its own row.
    const scale = groups.reduce(
      (largest, [, members]) => Math.max(largest, countNodes(members)),
      0,
    );
    // Rows are measured against the largest row, not against the largest group:
    // a 35-member class inside a 1140-symbol module would otherwise be a sliver
    // whatever its composition.
    const rowScale = rowScaleOf(groups);
    return html`<div>
      ${groups.map(
        ([name, members]) => html`
          <matrix-group
            class="mb-1.5 block overflow-hidden rounded-lg border border-zinc-200 dark:border-zinc-800"
            .name=${name}
            .side=${this.side}
            .members=${members}
            .links=${links}
            .matrix=${matrix}
            .scale=${scale}
            .rowScale=${rowScale}
            .target=${this.target?.group === name ? this.target : null}
          ></matrix-group>
        `,
      )}
    </div>`;
  }
}

customElements.define('matrix-app', MatrixAppElement);
