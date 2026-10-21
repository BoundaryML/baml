// biome-ignore-all lint/style/useFilenamingConvention: Preserve the existing public component filename.
import {
  Background,
  BackgroundVariant,
  Controls,
  type NodeMouseHandler,
  ReactFlow,
  ReactFlowProvider,
  useEdgesState,
  useNodesState,
  useReactFlow,
  useStore,
} from '@xyflow/react';
import {
  type FC,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import '@xyflow/react/dist/style.css';

import type { ResultRendererProps } from '../result-renderers';
import { runToGraphNodeValues } from '../run-store-projections';
import type { ValueBodyCache } from '../value-body-cache';
import type { ControlFlowGraph, Run } from '../worker-protocol';
import { getChrome } from './constants';
import { cfgToGraphNodes, graphToReactflow } from './convert';
import { ColorfulMarkerDefinitions, kEdgeTypes } from './edges';
import { layoutGraph } from './layout';
import {
  applyLevelOfDetail,
  computeNodeDepths,
  maxNodeDepth,
  zoomToRevealDepth,
} from './lod';
import { kNodeTypes } from './nodes';
import { GraphThemeContext, useGraphTheme } from './theme';
import type { NodeExecutionState, WorkflowEdge, WorkflowNode } from './types';
import {
  groupValuePreviewSourceNodeId,
  isGroupValuePreviewNode,
  liftGroupValuePreviews,
} from './value-previews';

interface GraphViewProps {
  graph: ControlFlowGraph;
  /** Function whose graph is displayed — keys the per-function layout
   *  direction memory. */
  functionName?: string | null;
  run?: Run | null;
  valueBodyCache?: ValueBodyCache;
  valueBodyCacheVersion?: number;
  runStatus?: Run['status'];
  runError?: string | null;
  customRenderers?: Record<string, FC<ResultRendererProps>>;
  selectedNodeId: number | null;
  onNodeClick: (nodeId: number) => void;
}

type LayoutDirection = 'horizontal' | 'vertical';

/** How the graph decides which subgraphs to reveal vs. collapse. */
type ExpandMode = 'zoom' | 'click' | 'all';

const EXPAND_MODES: { id: ExpandMode; label: string; title: string }[] = [
  { id: 'zoom', label: 'Zoom', title: 'Reveal subgraphs as you zoom in' },
  {
    id: 'click',
    label: 'Click',
    title: 'Shows the first 2 levels — click a node to expand deeper',
  },
  { id: 'all', label: 'All', title: 'Expand every subgraph' },
];

/** Click mode (the default) reveals this many levels up front; deeper
 *  subgraphs collapse to leaves until the user clicks one open. */
const CLICK_REVEAL_DEPTH = 2;

const DIRECTION_STORAGE_PREFIX = 'baml-graph-direction:';

/** Per-function layout direction, remembered across sessions. Vertical is
 *  the default until the user toggles. */
function storedDirection(
  functionName: string | null | undefined,
): LayoutDirection {
  if (typeof window === 'undefined') return 'vertical';
  try {
    const v = window.localStorage.getItem(
      DIRECTION_STORAGE_PREFIX + (functionName ?? ''),
    );
    return v === 'horizontal' || v === 'vertical' ? v : 'vertical';
  } catch {
    return 'vertical';
  }
}

function storeDirection(
  functionName: string | null | undefined,
  direction: LayoutDirection,
) {
  try {
    window.localStorage.setItem(
      DIRECTION_STORAGE_PREFIX + (functionName ?? ''),
      direction,
    );
  } catch {
    /* private browsing / quota — direction just won't persist */
  }
}

const WRAP_STORAGE_KEY = 'baml-graph-wrap';

/** Whether long chains wrap into rows to keep a bounded aspect ratio.
 *  Off = unbounded single row/column (full horizontal or vertical). Bounded
 *  is the default; remembered globally across functions and sessions. */
function storedWrap(): boolean {
  if (typeof window === 'undefined') return true;
  try {
    // Only an explicit opt-out disables wrapping; anything else stays bounded.
    return window.localStorage.getItem(WRAP_STORAGE_KEY) !== 'false';
  } catch {
    return true;
  }
}

function storeWrap(wrap: boolean) {
  try {
    window.localStorage.setItem(WRAP_STORAGE_KEY, wrap ? 'true' : 'false');
  } catch {
    /* private browsing / quota — preference just won't persist */
  }
}

interface GraphNodeRuntime {
  executionState: NodeExecutionState;
  errorMessage?: string | null;
}

function GraphViewInner({
  graph,
  functionName,
  run,
  valueBodyCache,
  valueBodyCacheVersion,
  runStatus,
  runError,
  customRenderers,
  selectedNodeId,
  onNodeClick,
}: GraphViewProps) {
  const theme = useGraphTheme();
  const chrome = getChrome(theme);
  const [nodes, setNodes, onNodesChange] = useNodesState<WorkflowNode>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<WorkflowEdge>([]);
  // The component is remounted (keyed) per function, so the lazy initializer
  // re-reads the remembered direction whenever the function changes.
  const [direction, setDirection] = useState<LayoutDirection>(() =>
    storedDirection(functionName),
  );
  // Whether long chains wrap into rows (bounded aspect ratio) or extend
  // unbounded in a single row/column. Remembered globally.
  const [wrap, setWrap] = useState<boolean>(() => storedWrap());
  // Set when the user toggles layout direction — the next completed layout
  // re-fits the viewport so the rotated graph is fully visible.
  const refitAfterLayoutRef = useRef(false);
  const selectedNodeIdRef = useRef(selectedNodeId);

  useEffect(() => {
    selectedNodeIdRef.current = selectedNodeId;
  }, [selectedNodeId]);

  const graphModel = useMemo(() => {
    const { nodes: graphNodes, edges: graphEdges } = cfgToGraphNodes(graph);
    const { nodes: rfNodes, edges: rfEdges } = graphToReactflow(
      graphNodes,
      graphEdges,
    );
    return { graphNodes, rfEdges, rfNodes };
    // `theme` is a dep so edge colors (baked in convert via getMarkerColors)
    // re-resolve when the surface theme flips.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [graph, theme]);

  // ── Level of detail: semantic zoom + click-to-expand ─────────────────────
  // The full CFG is deeply nested; we render only down to a reveal depth and
  // collapse deeper subgraphs into a single leaf. How that depth is chosen is
  // the "Expand" mode (bottom-right menu):
  //   • zoom  — depth follows the viewport zoom (semantic zoom)
  //   • click — first 2 levels shown; click a node to expand deeper
  //   • all   — everything expanded
  // `expanded` holds containers the user clicked open; it layers on any mode.
  const maxDepth = useMemo(
    () => maxNodeDepth(graphModel.rfNodes),
    [graphModel],
  );
  const [expandMode, setExpandMode] = useState<ExpandMode>('click');
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(
    () => new Set(),
  );
  // Enables node position/size transitions only after the first layout, so the
  // initial paint snaps into place instead of flying in from the origin.
  const [layoutReady, setLayoutReady] = useState(false);

  // Live viewport zoom (transform[2]). Driving the reveal depth off this — vs a
  // separate state updated in onMove — means the initial `fitView` already maps
  // to the right LOD, instead of showing a default depth until the first move.
  const viewportZoom = useStore((s) => s.transform[2]);
  const revealDepth =
    expandMode === 'all'
      ? // Reveal everything with a finite depth (not Infinity) so the LOD pass
        // still runs and stamps `data.depth` for depth-scaled layout/rendering.
        maxDepth + 1
      : expandMode === 'click'
        ? CLICK_REVEAL_DEPTH
        : zoomToRevealDepth(viewportZoom, maxDepth);

  const lodModel = useMemo(() => {
    const model = applyLevelOfDetail(graphModel.rfNodes, graphModel.rfEdges, {
      expanded,
      revealDepth,
    });
    return liftGroupValuePreviews(model.nodes, model.edges);
  }, [graphModel, revealDepth, expanded]);

  // Switching modes clears manual expansions and refits to the new extent.
  const selectExpandMode = useCallback((mode: ExpandMode) => {
    setExpandMode(mode);
    setExpanded(new Set());
    refitAfterLayoutRef.current = true;
  }, []);

  // Click a collapsed node to reveal its subgraph; click again to collapse.
  const toggleExpanded = useCallback((nodeId: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(nodeId)) next.delete(nodeId);
      else next.add(nodeId);
      return next;
    });
  }, []);

  const effectiveRunStatus = runStatus ?? run?.status;
  const effectiveRunError = runError ?? run?.error?.message ?? null;
  const rootGraphNodeId =
    graphModel.graphNodes.find((node) => node.type === 'function')?.id ?? null;
  const graphNodeValues = useMemo(
    () =>
      runToGraphNodeValues(run, valueBodyCache, {
        rootGraphNodeId,
      }),
    [run, valueBodyCache, valueBodyCacheVersion, rootGraphNodeId],
  );

  const runtimeInputsRef = useRef({
    customRenderers,
    graphNodeValues,
  });
  runtimeInputsRef.current = {
    customRenderers,
    graphNodeValues,
  };

  const graphNodesRef = useRef(graphModel.graphNodes);
  graphNodesRef.current = graphModel.graphNodes;

  const decorateNodesWithRuntime = useCallback(
    (baseNodes: WorkflowNode[]): WorkflowNode[] => {
      const {
        graphNodeValues: latestGraphNodeValues,
        customRenderers: latestCustomRenderers,
      } = runtimeInputsRef.current;
      // Per-node execution state needs a call-to-CFG-node mapping the run
      // store no longer carries. Until it is rebuilt from profiles-v1 call
      // sites, no node claims to have run or not run.
      const runtimeByNode = new Map<string, GraphNodeRuntime>();
      const selectedId =
        selectedNodeIdRef.current == null
          ? null
          : String(selectedNodeIdRef.current);

      return baseNodes.map((node) => {
        const runtime = runtimeByNode.get(node.id);
        const previewSourceNodeId = groupValuePreviewSourceNodeId(node);
        const valuePreviews = previewSourceNodeId
          ? (latestGraphNodeValues.get(previewSourceNodeId) ?? [])
          : node.data.groupValuePreviewsLifted
            ? []
            : (latestGraphNodeValues.get(node.id) ?? []);
        const executionState =
          previewSourceNodeId != null
            ? (runtimeByNode.get(previewSourceNodeId)?.executionState ??
              runtime?.executionState)
            : runtime?.executionState;
        const errorMessage =
          previewSourceNodeId != null
            ? (runtimeByNode.get(previewSourceNodeId)?.errorMessage ??
              runtime?.errorMessage)
            : runtime?.errorMessage;

        return {
          ...node,
          data: {
            ...node.data,
            customRenderers: latestCustomRenderers,
            errorMessage,
            executionState: executionState ?? ('not-started' as const),
            hasResult: undefined,
            result: undefined,
            selected:
              node.id === selectedId ||
              (previewSourceNodeId != null &&
                previewSourceNodeId === selectedId),
            valuePreviews,
          },
        };
      });
    },
    [],
  );

  /**
   * What about the value previews can change a node's SIZE.
   *
   * `graphNodeValues` is rebuilt whenever `run` changes identity, and the run
   * store returns a fresh object for every patch: one per payload, log line,
   * and status change. Keying layout on the map itself therefore restarted
   * ELK continuously for the whole of a run, which is what made the graph
   * flicker and blank while it was executing. Previews only alter geometry
   * when their number or rendered state changes, so that is what layout
   * watches; the decoration effect below still repaints on every update, so
   * values stay live without moving anything.
   */
  const graphNodeValuesGeometryKey = useMemo(() => {
    const parts: string[] = [];
    for (const [nodeId, values] of graphNodeValues) {
      parts.push(
        `${nodeId}:${values.length}:${values
          .map((value) => `${value.id}/${value.state}`)
          .join(',')}`,
      );
    }
    return parts.sort().join('|');
  }, [graphNodeValues]);

  const layoutRunIdRef = useRef(0);

  // Convert CFG -> ReactFlow and run layout when graph geometry changes.
  // Runtime overlay can affect geometry when an error preview is visible.
  useEffect(() => {
    const layoutRunId = ++layoutRunIdRef.current;
    const nodesWithRuntime = decorateNodesWithRuntime(lodModel.nodes);

    layoutGraph(nodesWithRuntime, lodModel.edges, direction, wrap)
      .then(({ nodes: laid, edges: laidEdges }) => {
        if (layoutRunId !== layoutRunIdRef.current) return;
        setNodes(decorateNodesWithRuntime(laid));
        setEdges(laidEdges);
        // Arm transitions for subsequent (expand/collapse) re-layouts.
        setLayoutReady(true);
        if (refitAfterLayoutRef.current) {
          refitAfterLayoutRef.current = false;
          // Let ReactFlow commit the re-laid nodes and re-measure their rotated
          // positions into the store before fitting — a couple of frames isn't
          // enough (fitView then sees the pre-rotation bounds and the toggled
          // graph looks uncentred). A short timeout matches the resize refit,
          // which settles reliably.
          setTimeout(() => {
            // Match the manual "fit view" control, which calls fitView() with no
            // options (padding 0.1, instance zoom range). Passing our own tighter
            // padding/maxZoom here left the toggled graph more zoomed out than
            // that button; defaults make it fit just as snugly.
            fitView({ duration: 250 });
          }, 150);
        }
      })
      .catch((err) => {
        console.error('[GraphView] Layout failed:', err);
      });
  }, [
    lodModel,
    effectiveRunStatus,
    effectiveRunError,
    graphNodeValuesGeometryKey,
    direction,
    wrap,
    setNodes,
    setEdges,
    decorateNodesWithRuntime,
  ]);

  // Keep the selected node visible under LOD: if it's currently hidden, open
  // its ancestor containers. Re-runs when the selection OR the graph/LOD model
  // changes, so a graph update that would hide the selection re-reveals it
  // (converges — once the ancestors are expanded the node stays visible).
  useEffect(() => {
    if (selectedNodeId == null) return;
    const id = String(selectedNodeId);
    if (lodModel.nodes.some((n) => n.id === id)) return;
    const parentById = new Map(
      graphModel.rfNodes.map((n) => [n.id, n.parentId]),
    );
    const depths = computeNodeDepths(graphModel.rfNodes);
    const ancestors: string[] = [];
    let cur = parentById.get(id);
    let guard = 0;
    while (cur != null && guard++ < 1000) {
      ancestors.push(cur);
      cur = parentById.get(cur);
    }
    // Only open ancestors that are actually closed at this zoom (depth past the
    // reveal threshold) and not already manually expanded — leave open ones be.
    const toOpen = ancestors.filter(
      (a) => (depths.get(a) ?? 0) >= revealDepth && !expanded.has(a),
    );
    if (toOpen.length === 0) return;
    setExpanded((prev) => {
      const next = new Set(prev);
      for (const a of toOpen) next.add(a);
      return next;
    });
  }, [selectedNodeId, lodModel, graphModel, revealDepth, expanded]);

  // Keep runtime node data fresh while a new ELK layout is pending.
  useEffect(() => {
    setNodes((nds) => decorateNodesWithRuntime(nds));
  }, [
    effectiveRunStatus,
    effectiveRunError,
    graphNodeValues,
    customRenderers,
    selectedNodeId,
    setNodes,
    decorateNodesWithRuntime,
  ]);

  // Auto-pan viewport to center the selected node — only when it's off-screen
  const { setCenter, getNode, getViewport, fitView } = useReactFlow();
  const containerWidth = useStore((s) => s.width);
  const containerHeight = useStore((s) => s.height);

  // Refit when ReactFlow's measured container size changes — e.g. dragging the
  // editor/graph splitter or resizing the window. ReactFlow keeps the viewport
  // transform across resizes, so without this the graph holds its old pan/zoom
  // and no longer fits the new size. Skip the first measurement (the `fitView`
  // prop handles initial paint) and debounce so we fit once the drag settles,
  // not on every intermediate width during the gesture.
  const didMeasureRef = useRef(false);
  useEffect(() => {
    if (!containerWidth || !containerHeight) return undefined;
    if (!didMeasureRef.current) {
      didMeasureRef.current = true;
      return undefined;
    }
    const t = setTimeout(() => {
      // Defaults (padding 0.1, instance zoom range) match the manual "fit view"
      // control so a resize fits as snugly as that button.
      fitView({ duration: 200 });
    }, 120);
    return () => clearTimeout(t);
  }, [containerWidth, containerHeight, fitView]);
  // Auto-pan to the selected node — but the node may not be laid out yet (e.g.
  // the LOD reveal effect above surfaces it only on a later layout). So record
  // the request on selection change, then fulfill it once the node actually
  // exists (the fulfill effect re-runs as `nodes` updates).
  const prevGraphRef = useRef(graph);
  const panRequestRef = useRef<string | null>(null);
  useEffect(() => {
    if (selectedNodeId == null) {
      panRequestRef.current = null;
      return;
    }
    // Skip auto-pan when the graph itself just changed (fitView handles that).
    if (prevGraphRef.current !== graph) {
      prevGraphRef.current = graph;
      panRequestRef.current = null;
      return;
    }
    panRequestRef.current = String(selectedNodeId);
  }, [selectedNodeId, graph]);

  useEffect(() => {
    const id = panRequestRef.current;
    if (id == null) return;
    const target = getNode(id);
    if (!target) return; // not laid out yet — a later layout re-runs this

    // Absolute position by walking up the parentId chain.
    let absX = target.position.x;
    let absY = target.position.y;
    let current = target;
    while (current.parentId) {
      const parent = getNode(current.parentId);
      if (!parent) break;
      absX += parent.position.x;
      absY += parent.position.y;
      current = parent;
    }

    const w = target.measured?.width ?? 150;
    const h = target.measured?.height ?? 40;
    const centerX = absX + w / 2;
    const centerY = absY + h / 2;

    const { x: vx, y: vy, zoom } = getViewport();
    const screenX = centerX * zoom + vx;
    const screenY = centerY * zoom + vy;
    const pad = 60;
    const isVisible =
      screenX >= pad &&
      screenX <= containerWidth - pad &&
      screenY >= pad &&
      screenY <= containerHeight - pad;
    if (!isVisible) {
      // Pan to the node; if over-zoomed, ease back to 1.0.
      const targetZoom = Math.min(zoom, 1.0);
      setCenter(centerX, centerY, { duration: 300, zoom: targetZoom });
    }
    panRequestRef.current = null; // fulfilled
  }, [
    nodes,
    selectedNodeId,
    setCenter,
    getNode,
    getViewport,
    containerWidth,
    containerHeight,
  ]);

  const handleNodeClick: NodeMouseHandler<WorkflowNode> = useCallback(
    (_event, node) => {
      // Click-to-expand / collapse: a collapsed node reveals its subgraph; a
      // node the user previously expanded collapses again. Both short-circuit
      // the normal navigate/select.
      if (node.data?.collapsed || expanded.has(node.id)) {
        toggleExpanded(node.id);
        return;
      }
      const nodeId = isGroupValuePreviewNode(node)
        ? (groupValuePreviewSourceNodeId(node) ?? node.id)
        : node.id;
      const numericNodeId = Number(nodeId);
      if (Number.isFinite(numericNodeId)) {
        onNodeClick(numericNodeId);
      }
    },
    [onNodeClick, expanded, toggleExpanded],
  );

  return (
    <GraphThemeContext.Provider value={theme}>
      <div
        className={
          layoutReady ? 'baml-graph baml-graph--animate' : 'baml-graph'
        }
        style={{ height: '100%', position: 'relative', width: '100%' }}
      >
        {/* Override @xyflow/react defaults:
            - .react-flow__node-group has a built-in light gray fill + 1px
              border (so nested groups stack into visible gray patches).
            - .react-flow__node-group.selected adds a square box-shadow halo
              on the (un-rounded) wrapper, which mismatches our rounded frame.
            - .react-flow__node:focus adds a browser outline on click.
          Strip all of those so our custom node styles render unobstructed. */}
        <style>{`
        .react-flow__node-group,
        .react-flow__node.parent {
          background: transparent !important;
          border: none !important;
          padding: 0 !important;
          box-shadow: none !important;
          border-radius: 12px !important;
        }
        .react-flow__node-group > div:first-child,
        .react-flow__node.parent > div:first-child {
          background: transparent !important;
        }
        .react-flow__node-group.selected,
        .react-flow__node.parent.selected {
          border: none !important;
          box-shadow: none !important;
        }
        /* Nodes draw their own selection ring (nodeShadow); suppress the
           wrapper's focus ring so selection doesn't render twice. */
        .react-flow__node:focus,
        .react-flow__node:focus-visible,
        .react-flow__node.selectable:focus,
        .react-flow__node.selectable:focus-visible {
          outline: none !important;
          box-shadow: none !important;
        }
        .react-flow.dark {
          --xy-controls-button-background-color-default: rgba(24, 24, 27, 0.92);
          --xy-controls-button-background-color-hover-default: rgba(39, 39, 42, 0.96);
          --xy-controls-button-color-default: #e4e4e7;
          --xy-controls-button-color-hover-default: #ffffff;
          --xy-controls-button-border-color-default: rgba(255, 255, 255, 0.10);
          --xy-controls-box-shadow-default: 0 8px 24px rgba(0, 0, 0, 0.24);
        }
        .react-flow.light {
          --xy-controls-button-background-color-default: rgba(255, 253, 246, 0.95);
          --xy-controls-button-background-color-hover-default: #f4eee0;
          --xy-controls-button-color-default: #1a1612;
          --xy-controls-button-color-hover-default: #1a1612;
          --xy-controls-button-border-color-default: #d8cfbd;
          --xy-controls-box-shadow-default: 0 4px 14px rgba(26, 22, 18, 0.10);
        }
        .react-flow__controls {
          border-radius: 8px;
          overflow: hidden;
          backdrop-filter: blur(8px);
          -webkit-backdrop-filter: blur(8px);
        }
        .react-flow.light .react-flow__controls {
          border: 1px solid rgba(26, 22, 18, 0.14);
        }
        .react-flow.dark .react-flow__controls {
          border: 1px solid rgba(255, 255, 255, 0.12);
        }
        @keyframes baml-graph-spin {
          to { transform: rotate(360deg); }
        }
        .react-flow__attribution { display: none !important; }
        /* Level-of-detail transitions: when expand/collapse re-layouts, nodes
           glide to their new spot and containers grow/shrink, and freshly
           revealed nodes fade in. Armed only after the first layout so the
           initial paint doesn't fly in from the origin. */
        .baml-graph--animate .react-flow__node {
          transition:
            transform 280ms cubic-bezier(0.4, 0, 0.2, 1),
            width 280ms cubic-bezier(0.4, 0, 0.2, 1),
            height 280ms cubic-bezier(0.4, 0, 0.2, 1);
        }
        .baml-graph--animate .react-flow__node {
          animation: baml-lod-in 280ms ease both;
        }
        @keyframes baml-lod-in {
          from { opacity: 0; }
          to { opacity: 1; }
        }
        @media (prefers-reduced-motion: reduce) {
          .baml-graph--animate .react-flow__node {
            transition: none;
            animation: none;
          }
        }
      `}</style>
        <ReactFlow
          colorMode={theme}
          edges={edges}
          edgesFocusable={false}
          edgeTypes={kEdgeTypes}
          elementsSelectable
          elevateEdgesOnSelect={false}
          elevateNodesOnSelect={false}
          fitView
          fitViewOptions={{ maxZoom: 1.5, minZoom: 0.3, padding: 0.2 }}
          nodes={nodes}
          nodesDraggable={false}
          nodesFocusable
          nodeTypes={kNodeTypes}
          onEdgesChange={onEdgesChange}
          onNodeClick={handleNodeClick}
          onNodesChange={onNodesChange}
          panActivationKeyCode={null}
          panOnDrag={[0, 1, 2]}
          panOnScroll
          proOptions={{ hideAttribution: true }}
          selectNodesOnDrag={false}
        >
          <Controls
            position="bottom-left"
            style={{ display: 'flex', flexDirection: 'row' }}
          />
          <Background
            color={chrome.backgroundDots}
            gap={18}
            size={1}
            variant={BackgroundVariant.Dots}
          />
          <ColorfulMarkerDefinitions />
        </ReactFlow>
        <button
          aria-label="Toggle layout direction"
          onClick={() => {
            refitAfterLayoutRef.current = true;
            setDirection((d) => {
              const next = d === 'horizontal' ? 'vertical' : 'horizontal';
              storeDirection(functionName, next);
              return next;
            });
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = chrome.button.bgHover;
            e.currentTarget.style.borderColor = chrome.button.borderHover;
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = chrome.button.bg;
            e.currentTarget.style.borderColor = chrome.button.border;
          }}
          style={{
            alignItems: 'center',
            backdropFilter: 'blur(8px)',
            background: chrome.button.bg,
            border: `1px solid ${chrome.button.border}`,
            borderRadius: 8,
            boxShadow: chrome.button.shadow,
            color: chrome.button.text,
            cursor: 'pointer',
            display: 'flex',
            fontSize: 14,
            height: 30,
            justifyContent: 'center',
            lineHeight: 1,
            padding: 0,
            position: 'absolute',
            right: 10,
            top: 10,
            transition: 'background 120ms ease, border-color 120ms ease',
            WebkitBackdropFilter: 'blur(8px)',
            width: 30,
            zIndex: 10,
          }}
          title={`Switch to ${direction === 'horizontal' ? 'vertical' : 'horizontal'} layout`}
          type="button"
        >
          {direction === 'horizontal' ? '\u2195' : '\u2194'}
        </button>
        {/* Bottom-right layout controls: aspect-ratio wrap toggle + expand mode. */}
        <div
          style={{
            alignItems: 'center',
            bottom: 10,
            display: 'flex',
            gap: 8,
            position: 'absolute',
            right: 10,
            zIndex: 10,
          }}
        >
          {/* Wrap toggle: bounded aspect ratio (chain wraps into rows) vs.
            unbounded — full horizontal / full vertical single line. */}
          <button
            aria-checked={wrap}
            aria-label="Wrap long chains into rows"
            onClick={() => {
              refitAfterLayoutRef.current = true;
              setWrap((w) => {
                const next = !w;
                storeWrap(next);
                return next;
              });
            }}
            role="switch"
            style={{
              alignItems: 'center',
              backdropFilter: 'blur(8px)',
              background: chrome.button.bg,
              border: `1px solid ${wrap ? chrome.selectionRing.color : chrome.button.border}`,
              borderRadius: 10,
              boxShadow: chrome.button.shadow,
              color: wrap ? chrome.selectionRing.color : chrome.button.text,
              cursor: 'pointer',
              display: 'flex',
              fontFamily:
                'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
              fontSize: 11.5,
              fontWeight: wrap ? 700 : 500,
              gap: 5,
              padding: '5px 10px',
              transition: 'color 120ms ease, border-color 120ms ease',
              WebkitBackdropFilter: 'blur(8px)',
            }}
            title={
              wrap
                ? 'Bounded: long chains wrap into rows. Click for unbounded (full horizontal/vertical).'
                : 'Unbounded: full horizontal/vertical. Click to wrap long chains into rows.'
            }
            type="button"
          >
            <span aria-hidden="true" style={{ fontSize: 13, lineHeight: 1 }}>
              {wrap ? '↵' : '→'}
            </span>
            Wrap
          </button>
          {/* Expand mode: how subgraphs are revealed (semantic zoom / click / all). */}
          <div
            aria-label="Subgraph expand mode"
            role="radiogroup"
            style={{
              alignItems: 'center',
              backdropFilter: 'blur(8px)',
              background: chrome.button.bg,
              border: `1px solid ${chrome.button.border}`,
              borderRadius: 10,
              boxShadow: chrome.button.shadow,
              display: 'flex',
              fontFamily:
                'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
              gap: 4,
              padding: '4px 6px',
              WebkitBackdropFilter: 'blur(8px)',
            }}
          >
            <span
              style={{
                color: chrome.button.text,
                fontSize: 10,
                fontWeight: 600,
                letterSpacing: '0.05em',
                opacity: 0.55,
                padding: '0 2px',
                textTransform: 'uppercase',
              }}
            >
              Expand
            </span>
            {EXPAND_MODES.map((m) => {
              const on = expandMode === m.id;
              return (
                <button
                  aria-pressed={on}
                  key={m.id}
                  onClick={() => selectExpandMode(m.id)}
                  style={{
                    background: 'transparent',
                    border: `1px solid ${on ? chrome.selectionRing.color : 'transparent'}`,
                    borderRadius: 7,
                    color: on ? chrome.selectionRing.color : chrome.button.text,
                    cursor: 'pointer',
                    fontSize: 11.5,
                    fontWeight: on ? 700 : 500,
                    padding: '3px 9px',
                    transition: 'color 120ms ease, border-color 120ms ease',
                  }}
                  title={m.title}
                  type="button"
                >
                  {m.label}
                </button>
              );
            })}
          </div>
        </div>
      </div>
    </GraphThemeContext.Provider>
  );
}

export function GraphView(props: GraphViewProps) {
  return (
    <ReactFlowProvider>
      {/* Keyed per function so the remembered layout direction is re-read
          when the displayed function changes. */}
      <GraphViewInner key={props.functionName ?? ''} {...props} />
    </ReactFlowProvider>
  );
}
