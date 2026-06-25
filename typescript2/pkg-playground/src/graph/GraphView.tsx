import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FC,
} from 'react';
import {
  ReactFlow,
  ReactFlowProvider,
  useNodesState,
  useEdgesState,
  useReactFlow,
  useStore,
  Controls,
  Background,
  BackgroundVariant,
  type NodeMouseHandler,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';

import type {
  CallNode,
  ControlFlowGraph,
  GraphRuntimeOverlay,
  Run,
} from '../worker-protocol';
import type { ResultRendererProps } from '../result-renderers';
import { getChrome } from './constants';
import { cfgToGraphNodes, graphToReactflow } from './convert';
import { layoutGraph } from './layout';
import { applyLevelOfDetail, maxNodeDepth, zoomToRevealDepth } from './lod';
import { kNodeTypes } from './nodes';
import { kEdgeTypes, ColorfulMarkerDefinitions } from './edges';
import { GraphThemeContext, useGraphTheme } from './theme';
import type {
  GraphNode,
  NodeExecutionState,
  WorkflowNode,
  WorkflowEdge,
} from './types';

interface GraphViewProps {
  graph: ControlFlowGraph;
  /** Function whose graph is displayed — keys the per-function layout
   *  direction memory. */
  functionName?: string | null;
  graphRuntimeOverlay?: GraphRuntimeOverlay | null;
  calls?: CallNode[];
  runStatus?: Run['status'];
  runError?: string | null;
  customRenderers?: Record<string, FC<ResultRendererProps>>;
  selectedNodeId: number | null;
  onNodeClick: (nodeId: number) => void;
}

const EMPTY_CALLS: CallNode[] = [];

type LayoutDirection = 'horizontal' | 'vertical';

const DIRECTION_STORAGE_PREFIX = 'baml-graph-direction:';

/** Per-function layout direction, remembered across sessions. Vertical is
 *  the default until the user toggles. */
function storedDirection(functionName: string | null | undefined): LayoutDirection {
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

interface GraphNodeRuntime {
  executionState: NodeExecutionState;
  errorMessage?: string | null;
}

const statePriority: Record<NodeExecutionState, number> = {
  'not-started': 0,
  skipped: 0,
  pending: 1,
  cached: 2,
  success: 3,
  cancelled: 4,
  running: 5,
  error: 6,
};

function mergeState(
  current: NodeExecutionState | undefined,
  next: NodeExecutionState,
): NodeExecutionState {
  if (current == null) return next;
  return statePriority[next] > statePriority[current] ? next : current;
}

function terminalRunState(status?: Run['status']): NodeExecutionState | null {
  switch (status) {
    case 'failed':
    case 'panicked':
      return 'error';
    case 'cancelled':
      return 'cancelled';
    case 'succeeded':
      return 'success';
    default:
      return null;
  }
}

function callExecutionState(
  call: CallNode,
  runStatus?: Run['status'],
): NodeExecutionState {
  const terminal = terminalRunState(runStatus);
  switch (call.status) {
    case 'running':
      return terminal ?? 'running';
    case 'ok':
    case 'exited':
      return 'success';
    case 'errored':
      return 'error';
    case 'cancelled':
      return 'cancelled';
    default:
      call.status satisfies never;
      return terminal ?? 'not-started';
  }
}

function collectOverlayNodeRuntime(
  graphNodes: GraphNode[],
  overlay: GraphRuntimeOverlay | null | undefined,
  calls: CallNode[],
  runStatus?: Run['status'],
  runError?: string | null,
): Map<string, GraphNodeRuntime> {
  if (!overlay || overlay.entries.length === 0 || calls.length === 0) {
    return new Map();
  }

  const callById = new Map(calls.map((call) => [call.id, call]));
  const parentById = new Map(graphNodes.map((node) => [node.id, node.parent]));
  const direct = new Map<string, GraphNodeRuntime>();

  for (const entry of overlay.entries) {
    const nodeId = String(entry.cfgNodeId);
    let executionState: NodeExecutionState | undefined;
    let hasError = false;

    for (const callNodeId of entry.callNodeIds) {
      const call = callById.get(callNodeId);
      if (!call) continue;
      const callState = callExecutionState(call, runStatus);
      executionState = mergeState(executionState, callState);
      hasError = hasError || callState === 'error';
    }

    if (!executionState) continue;
    direct.set(nodeId, {
      executionState,
      errorMessage: hasError ? (runError ?? undefined) : undefined,
    });
  }

  const withAncestors = new Map(direct);
  for (const [nodeId, runtime] of direct) {
    let parentId = parentById.get(nodeId);
    while (parentId != null) {
      const prev = withAncestors.get(parentId);
      withAncestors.set(parentId, {
        executionState: mergeState(prev?.executionState, runtime.executionState),
        errorMessage: runtime.errorMessage ?? prev?.errorMessage,
      });
      parentId = parentById.get(parentId);
    }
  }

  return withAncestors;
}

function GraphViewInner({
  graph,
  functionName,
  graphRuntimeOverlay,
  calls = EMPTY_CALLS,
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
    return { graphNodes, rfNodes, rfEdges };
    // `theme` is a dep so edge colors (baked in convert via getMarkerColors)
    // re-resolve when the surface theme flips.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [graph, theme]);

  // ── Semantic ("Google Maps") zoom ────────────────────────────────────────
  // The full CFG is deeply nested; we only render down to `revealDepth`,
  // collapsing deeper subgraphs into a single leaf. The depth tracks the
  // viewport zoom (see onMove below): zoom out → shallow, zoom in → deeper.
  const maxDepth = useMemo(
    () => maxNodeDepth(graphModel.rfNodes),
    [graphModel],
  );
  const [revealDepth, setRevealDepth] = useState(2);
  const lodModel = useMemo(
    () =>
      applyLevelOfDetail(graphModel.rfNodes, graphModel.rfEdges, revealDepth),
    [graphModel, revealDepth],
  );

  // Map the live zoom to a reveal depth. State only changes at depth
  // boundaries, so this re-layouts at most a few times per zoom gesture.
  const onMove = useCallback(
    (_event: unknown, viewport: { zoom: number }) => {
      setRevealDepth((prev) => {
        const next = zoomToRevealDepth(viewport.zoom, maxDepth);
        return next === prev ? prev : next;
      });
    },
    [maxDepth],
  );

  const runtimeInputsRef = useRef({
    graphRuntimeOverlay,
    calls,
    runStatus,
    runError,
    customRenderers,
  });
  runtimeInputsRef.current = {
    graphRuntimeOverlay,
    calls,
    runStatus,
    runError,
    customRenderers,
  };

  const graphNodesRef = useRef(graphModel.graphNodes);
  graphNodesRef.current = graphModel.graphNodes;

  const decorateNodesWithRuntime = useCallback(
    (baseNodes: WorkflowNode[]): WorkflowNode[] => {
      const {
        graphRuntimeOverlay: latestGraphRuntimeOverlay,
        calls: latestCalls,
        runStatus: latestRunStatus,
        runError: latestRunError,
        customRenderers: latestCustomRenderers,
      } = runtimeInputsRef.current;
      const runtimeByNode = collectOverlayNodeRuntime(
        graphNodesRef.current,
        latestGraphRuntimeOverlay,
        latestCalls,
        latestRunStatus,
        latestRunError,
      );
      const selectedId =
        selectedNodeIdRef.current == null
          ? null
          : String(selectedNodeIdRef.current);

      return baseNodes.map((node) => {
        const runtime = runtimeByNode.get(node.id);
        if (!runtime) {
          return {
            ...node,
            data: {
              ...node.data,
              result: undefined,
              hasResult: undefined,
              imageOutputs: [],
              executionState: 'not-started' as const,
              errorMessage: undefined,
              customRenderers: latestCustomRenderers,
              selected: node.id === selectedId,
            },
          };
        }

        return {
          ...node,
          data: {
            ...node.data,
            result: undefined,
            hasResult: undefined,
            imageOutputs: [],
            executionState: runtime.executionState,
            errorMessage: runtime.errorMessage,
            customRenderers: latestCustomRenderers,
            selected: node.id === selectedId,
          },
        };
      });
    },
    [],
  );

  const layoutRunIdRef = useRef(0);

  // Convert CFG -> ReactFlow and run layout when graph geometry changes.
  // Runtime overlay can affect geometry when an error preview is visible.
  useEffect(() => {
    const layoutRunId = ++layoutRunIdRef.current;
    const nodesWithRuntime = decorateNodesWithRuntime(lodModel.nodes);

    layoutGraph(nodesWithRuntime, lodModel.edges, direction)
      .then(({ nodes: laid, edges: laidEdges }) => {
        if (layoutRunId !== layoutRunIdRef.current) return;
        setNodes(decorateNodesWithRuntime(laid));
        setEdges(laidEdges);
        if (refitAfterLayoutRef.current) {
          refitAfterLayoutRef.current = false;
          // Wait a frame so ReactFlow has measured the re-laid nodes.
          requestAnimationFrame(() => {
            fitView({ padding: 0.2, minZoom: 0.3, maxZoom: 1.5, duration: 250 });
          });
        }
      })
      .catch((err) => {
        console.error('[GraphView] Layout failed:', err);
      });
  }, [
    lodModel,
    graphRuntimeOverlay,
    calls,
    runStatus,
    runError,
    direction,
    setNodes,
    setEdges,
    decorateNodesWithRuntime,
  ]);

  // Keep runtime node data fresh while a new ELK layout is pending.
  useEffect(() => {
    setNodes((nds) => decorateNodesWithRuntime(nds));
  }, [
    graphRuntimeOverlay,
    calls,
    runStatus,
    runError,
    customRenderers,
    selectedNodeId,
    setNodes,
    decorateNodesWithRuntime,
  ]);

  // Update selected state on nodes
  useEffect(() => {
    setNodes((nds) =>
      nds.map((n) => ({
        ...n,
        data: { ...n.data, selected: n.id === String(selectedNodeId) },
      })),
    );
  }, [selectedNodeId, setNodes]);

  // Auto-pan viewport to center the selected node — only when it's off-screen
  const { setCenter, getNode, getViewport, fitView } = useReactFlow();
  const containerWidth = useStore((s) => s.width);
  const containerHeight = useStore((s) => s.height);
  const prevGraphRef = useRef(graph);
  useEffect(() => {
    if (selectedNodeId == null) return;
    // Skip auto-pan when the graph itself just changed (fitView handles that)
    if (prevGraphRef.current !== graph) {
      prevGraphRef.current = graph;
      return;
    }
    const target = getNode(String(selectedNodeId));
    if (!target) return;

    // Compute absolute position by walking up parentId chain
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

    // Check if node center is already visible in the viewport
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
      // Pan to the node; if over-zoomed, ease back to 1.0
      const targetZoom = Math.min(zoom, 1.0);
      setCenter(centerX, centerY, { duration: 300, zoom: targetZoom });
    }
  }, [
    selectedNodeId,
    graph,
    setCenter,
    getNode,
    getViewport,
    containerWidth,
    containerHeight,
  ]);

  const handleNodeClick: NodeMouseHandler<WorkflowNode> = useCallback(
    (_event, node) => {
      onNodeClick(Number(node.id));
    },
    [onNodeClick],
  );

  return (
    <GraphThemeContext.Provider value={theme}>
    <div style={{ width: '100%', height: '100%', position: 'relative' }}>
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
      `}</style>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        nodeTypes={kNodeTypes}
        edgeTypes={kEdgeTypes}
        onNodeClick={handleNodeClick}
        onMove={onMove}
        nodesDraggable={false}
        nodesFocusable
        edgesFocusable={false}
        elementsSelectable
        selectNodesOnDrag={false}
        elevateNodesOnSelect={false}
        elevateEdgesOnSelect={false}
        panOnDrag={[0, 1, 2]}
        panOnScroll
        panActivationKeyCode={null}
        fitView
        fitViewOptions={{ minZoom: 0.3, maxZoom: 1.5, padding: 0.2 }}
        proOptions={{ hideAttribution: true }}
        colorMode={theme}
      >
        <Controls
          position="bottom-left"
          style={{ display: 'flex', flexDirection: 'row' }}
        />
        <Background
          variant={BackgroundVariant.Dots}
          color={chrome.backgroundDots}
          gap={18}
          size={1}
        />
        <ColorfulMarkerDefinitions />
      </ReactFlow>
      <button
        onClick={() => {
          refitAfterLayoutRef.current = true;
          setDirection((d) => {
            const next = d === 'horizontal' ? 'vertical' : 'horizontal';
            storeDirection(functionName, next);
            return next;
          });
        }}
        style={{
          position: 'absolute',
          top: 10,
          right: 10,
          zIndex: 10,
          width: 30,
          height: 30,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          padding: 0,
          borderRadius: 8,
          border: `1px solid ${chrome.button.border}`,
          background: chrome.button.bg,
          backdropFilter: 'blur(8px)',
          WebkitBackdropFilter: 'blur(8px)',
          color: chrome.button.text,
          cursor: 'pointer',
          fontSize: 14,
          lineHeight: 1,
          boxShadow: chrome.button.shadow,
          transition: 'background 120ms ease, border-color 120ms ease',
        }}
        onMouseEnter={(e) => {
          e.currentTarget.style.background = chrome.button.bgHover;
          e.currentTarget.style.borderColor = chrome.button.borderHover;
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.background = chrome.button.bg;
          e.currentTarget.style.borderColor = chrome.button.border;
        }}
        title={`Switch to ${direction === 'horizontal' ? 'vertical' : 'horizontal'} layout`}
        aria-label="Toggle layout direction"
      >
        {direction === 'horizontal' ? '\u2195' : '\u2194'}
      </button>
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
