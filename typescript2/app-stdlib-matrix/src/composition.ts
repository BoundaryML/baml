import { html, nothing, type TemplateResult } from 'lit';
import {
  countNodes,
  type Links,
  type Side,
  type SymbolState,
  stateOf,
  type TreeNode,
} from './types';

// How a subtree came out, as a bar.
//
// Shared by the group headers and by any row that holds members, so a module
// and a class inside it read the same way. Both answer the same question —
// "how much surface is here, and how much of it is answered" — and answering it
// in two visual languages would make them incomparable for no reason.

/**
 * The bar's segments, in reading order: answered, answered differently,
 * answered by not arising, answered with a "no", and unanswered.
 *
 * The same visual vocabulary as the row swatches, so the legend explains both,
 * but flat classes rather than the `swatch-*` utilities — those carry a border
 * and a radius that a slice this thin cannot afford.
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
  // `unnecessary` sits past `none`: both say there is no counterpart, and the
  // difference between them is why. Reading left to right the bar goes from
  // answered, through answered-differently, to not-there, to not-needed.
  //
  // The BAML side has neither — an absence names no BAML symbol, and
  // `unnecessary` folds into `match` there (see `stateOf`) — so its solid
  // segment is the unnamed one.
  return side === 'ts'
    ? [
        ...shared,
        ['none', 'seg-ts', 'no BAML counterpart'],
        ['unnecessary', 'seg-unnecessary', 'unnecessary in BAML'],
        ['unjudged', 'seg-unjudged', 'not yet judged'],
      ]
    : [...shared, ['unjudged', 'seg-baml', 'no judgement names it']];
}

/**
 * How a subtree's symbols came out, counting every descendant.
 *
 * Members nest under their container, so counting only the top level would
 * describe a module by its type names rather than by its surface — `node:os`
 * would read as 20 and `(globals)` as 58, when the second holds twenty times
 * the API.
 */
export function tally(
  nodes: TreeNode[],
  links: Links,
  side: Side,
): Record<SymbolState, number> {
  const counts: Record<SymbolState, number> = {
    divergent: 0,
    match: 0,
    none: 0,
    unjudged: 0,
    unnecessary: 0,
  };
  const walk = (subtree: TreeNode[]) => {
    for (const node of subtree) {
      counts[stateOf(links, side, node.index)] += 1;
      walk(node.children);
    }
  };
  walk(nodes);
  return counts;
}

/**
 * A count and a bar whose length is this subtree's share of `scale`.
 *
 * Numbers alone made everything look alike: `(globals)` holds 1140 symbols and
 * `node:querystring` holds 4, and both rendered as two short counts. Length
 * carries the scale and the segments carry the breakdown, so a reader sees at a
 * glance both how much surface is here and how much of it is answered.
 *
 * `scale` is the largest peer's size, and peers are compared only against peers
 * — groups against groups, member-holding rows against member-holding rows. A
 * class with 35 members measured against a module with 1140 would be a sliver
 * whatever its composition, which answers no question anyone has.
 */
export function compositionBar(
  nodes: TreeNode[],
  links: Links,
  side: Side,
  scale: number,
  size: 'group' | 'row',
): TemplateResult {
  const counts = tally(nodes, links, side);
  const total = countNodes(nodes);
  const share = scale > 0 ? (total / scale) * 100 : 0;
  const track = size === 'group' ? 'w-40' : 'w-24';
  const height = size === 'group' ? 'h-2' : 'h-1.5';
  return html`<span class="flex shrink-0 items-center gap-2">
    <span class="w-10 text-right text-xs text-zinc-500 tabular-nums">${total}</span>
    <span class="flex ${track} self-center"
      ><span
        class="flex ${height} overflow-hidden rounded-full"
        style=${`width: ${Math.max(share, 1.5)}%`}
        >${segmentsFor(side).map(([state, cls, label]) =>
          counts[state] === 0
            ? nothing
            : html`<span
                class=${cls}
                style=${`flex: ${counts[state]}`}
                title=${`${counts[state]} ${label}`}
              ></span>`,
        )}</span
      ></span
    >
  </span>`;
}

/**
 * The largest member-holding row anywhere in a view.
 *
 * Computed over the whole side rather than per group so that two classes in
 * different modules stay comparable — the eye reads down the page, not only
 * within one open group.
 */
export function rowScaleOf(groups: Array<[string, TreeNode[]]>): number {
  let largest = 0;
  const walk = (nodes: TreeNode[]) => {
    for (const node of nodes) {
      if (node.children.length > 0) {
        largest = Math.max(largest, countNodes(node.children));
      }
      walk(node.children);
    }
  };
  for (const [, members] of groups) walk(members);
  return largest;
}
