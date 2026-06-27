/**
 * Semantic ("Google Maps") zoom for the control-flow graph.
 *
 * The CFG is deeply nested (function → header → loop → call expansion → if →
 * arms → …). Rendering every level at once is overwhelming when zoomed out.
 * Level-of-detail (LOD) collapses everything below a `revealDepth`: nodes
 * deeper than the threshold are removed, the container at the boundary renders
 * as a single leaf summarizing its hidden subgraph, and edges that crossed into
 * the hidden region are re-pointed to that boundary node.
 *
 * `revealDepth` is driven by the viewport zoom (see `zoomToRevealDepth`): zoom
 * out → shallow depth (few, high-level nodes); zoom in → deeper, more detail.
 *
 * This module is pure (no React / React Flow runtime) so it is unit-testable;
 * GraphView wires it to the live viewport.
 */

import type { WorkflowEdge, WorkflowNode } from './types';

/** Depth of every node = number of container ancestors (root nodes are 0). */
export function computeNodeDepths(
  nodes: Pick<WorkflowNode, 'id' | 'parentId'>[],
): Map<string, number> {
  const parentOf = new Map<string, string | undefined>();
  for (const n of nodes) parentOf.set(n.id, n.parentId ?? undefined);

  const depth = new Map<string, number>();
  const resolve = (id: string, seen: Set<string>): number => {
    const cached = depth.get(id);
    if (cached != null) return cached;
    const parent = parentOf.get(id);
    // Unknown/cyclic parent → treat as a root so we never loop forever.
    const d =
      parent == null || !parentOf.has(parent) || seen.has(id)
        ? 0
        : resolve(parent, new Set(seen).add(id)) + 1;
    depth.set(id, d);
    return d;
  };
  for (const n of nodes) resolve(n.id, new Set());
  return depth;
}

/** The deepest nesting level present (0 if flat/empty). */
export function maxNodeDepth(nodes: Pick<WorkflowNode, 'id' | 'parentId'>[]): number {
  let max = 0;
  for (const d of computeNodeDepths(nodes).values()) max = Math.max(max, d);
  return max;
}

/**
 * Map a viewport zoom to a reveal depth. Below `zoomLo` only the first level
 * inside the function shows; at/above `zoomHi` the whole graph is expanded;
 * in between it scales linearly. Always returns at least 1 so the function's
 * immediate steps are visible even when fully zoomed out.
 */
export function zoomToRevealDepth(
  zoom: number,
  maxDepth: number,
  zoomLo = 0.5,
  zoomHi = 1.4,
): number {
  if (maxDepth <= 1) return maxDepth;
  if (zoom <= zoomLo) return 1;
  if (zoom >= zoomHi) return maxDepth;
  const t = (zoom - zoomLo) / (zoomHi - zoomLo);
  const depth = 1 + Math.round(t * (maxDepth - 1));
  return Math.min(maxDepth, Math.max(1, depth));
}

export interface LodOptions {
  /**
   * Containers shallower than this reveal their children automatically. Use
   * `Infinity` to expand everything, `1` to collapse to the function's first
   * level (then drive detail purely via `expanded` / click-to-expand).
   */
  revealDepth: number;
  /**
   * Container ids the user manually expanded. They reveal their children
   * regardless of `revealDepth`, so zoom-driven and click-driven expansion
   * share one mechanism.
   */
  expanded?: ReadonlySet<string>;
}

const EMPTY_SET: ReadonlySet<string> = new Set();

/**
 * Font/size multiplier for a node at the given nesting depth. Deeper nodes
 * render smaller so revealed detail reads as subordinate (and the viewport
 * zoom then magnifies it toward readability). Clamped so deep nodes stay legible.
 */
export function depthScale(depth: number): number {
  return Math.max(0.7, 1 - Math.max(0, depth) * 0.1);
}

/**
 * Collapse the graph to the requested level of detail:
 * - a container reveals its children when it's shallower than `revealDepth`
 *   OR it's in `expanded`; otherwise its subtree is hidden,
 * - a hidden subtree's boundary container becomes a leaf
 *   (`type: 'base'`, `data.collapsed: true`, `data.collapsedCount: N`),
 * - edges crossing a collapse boundary are re-pointed to the boundary node and
 *   edges that became internal to a single collapsed node are dropped.
 */
export function applyLevelOfDetail(
  nodes: WorkflowNode[],
  edges: WorkflowEdge[],
  options: LodOptions,
): { nodes: WorkflowNode[]; edges: WorkflowEdge[] } {
  const { revealDepth, expanded = EMPTY_SET } = options;
  if (!Number.isFinite(revealDepth) && expanded.size === 0) {
    return { nodes, edges };
  }

  const depth = computeNodeDepths(nodes);
  const byId = new Map(nodes.map((n) => [n.id, n]));

  // A container reveals its children when shallow enough or explicitly opened.
  const isOpen = (id: string) =>
    (depth.get(id) ?? 0) < revealDepth || expanded.has(id);

  // A node is visible iff every ancestor container is open.
  const visibleMemo = new Map<string, boolean>();
  const isVisible = (id: string): boolean => {
    const cached = visibleMemo.get(id);
    if (cached != null) return cached;
    const parent = byId.get(id)?.parentId;
    let v: boolean;
    if (parent == null || !byId.has(parent)) {
      v = true; // root
    } else {
      visibleMemo.set(id, true); // cycle guard while recursing
      v = isVisible(parent) && isOpen(parent);
    }
    visibleMemo.set(id, v);
    return v;
  };

  // Nearest visible ancestor (the node itself if already visible).
  const resolveVisible = (id: string): string => {
    let cur: string | undefined = id;
    let guard = 0;
    while (cur != null && !isVisible(cur) && guard++ < 10_000) {
      cur = byId.get(cur)?.parentId;
    }
    return cur ?? id;
  };

  // Count hidden descendants per boundary node (for the "+N" affordance).
  const hiddenCount = new Map<string, number>();
  for (const n of nodes) {
    if (isVisible(n.id)) continue;
    const boundary = resolveVisible(n.id);
    hiddenCount.set(boundary, (hiddenCount.get(boundary) ?? 0) + 1);
  }

  const outNodes: WorkflowNode[] = [];
  for (const n of nodes) {
    if (!isVisible(n.id)) continue;
    // Nesting depth rides on the node so the renderer can size deeper nodes
    // smaller (semantic-zoom feel: revealed detail starts small, then the
    // viewport zoom magnifies it toward readability).
    const d = depth.get(n.id) ?? 0;
    const hidden = hiddenCount.get(n.id) ?? 0;
    if (n.type === 'group' && hidden > 0) {
      // Boundary container: render as a leaf summarizing the hidden subgraph.
      outNodes.push({
        ...n,
        type: 'base',
        data: { ...n.data, depth: d, collapsed: true, collapsedCount: hidden },
      });
    } else if (n.type === 'group' && expanded.has(n.id)) {
      // Container the user opened via click — mark it collapsible for the UI.
      outNodes.push({ ...n, data: { ...n.data, depth: d, expanded: true } });
    } else {
      outNodes.push({ ...n, data: { ...n.data, depth: d } });
    }
  }

  const seen = new Set<string>();
  const outEdges: WorkflowEdge[] = [];
  for (const e of edges) {
    const source = resolveVisible(e.source);
    const target = resolveVisible(e.target);
    if (source === target) continue; // collapsed into one node
    const key = `${source}\0${target}`;
    if (seen.has(key)) continue;
    seen.add(key);
    outEdges.push({ ...e, source, target });
  }

  return { nodes: outNodes, edges: outEdges };
}
