'use client';

import React, { useState, useCallback, useEffect } from 'react';
import { Play, PlayCircle } from 'lucide-react';
import { ResizablePanelGroup, ResizablePanel, ResizableHandle } from '@baml/ui/resizable';
import { cn } from '@baml/ui/lib/utils';
import {
  ReactFlow,
  ReactFlowProvider,
  Node,
  Edge,
  MarkerType,
  useNodesState,
  useEdgesState,
  Position,
  Handle,
  useReactFlow,
} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import { hierarchy, tree } from 'd3-hierarchy';

// Mock trace data
interface TraceSpan {
  id: string;
  name: string;
  depth?: number;
  children?: TraceSpan[];
  input?: any;
  output?: any;
  latency?: number;
  cost?: number;
  metadata?: Record<string, any>;
}

interface TraceRun {
  id: string;
  timestamp: string;
  status: 'success' | 'error' | 'partial';
  totalLatency: number;
  totalCost: number;
  trace: TraceSpan;
}

const mockTraceRuns: TraceRun[] = [
  {
    id: 'run-1',
    timestamp: '2025-09-30 14:23:45',
    status: 'success',
    totalLatency: 1250,
    totalCost: 0.0042,
    trace: {
      id: 'root-1',
      name: 'Root',
      depth: 0,
      input: { query: 'What is the weather today?', context: { user_id: '123' } },
      output: { result: 'The weather is sunny with a high of 75°F', confidence: 0.95 },
      latency: 1250,
      cost: 0.0042,
      metadata: { model: 'gpt-4', temperature: 0.7 },
      children: [
        {
          id: 'llma-1',
          name: 'ExtractIntent',
          depth: 1,
          input: { prompt: 'Extract weather query intent', text: 'What is the weather today?' },
          output: { intent: 'weather_query', location: 'current' },
          latency: 450,
          cost: 0.0015,
          metadata: { model: 'gpt-3.5-turbo', tokens: 125 },
        },
        {
          id: 'llmb-1',
          name: 'GetWeather',
          depth: 1,
          input: { intent: 'weather_query', location: 'current' },
          output: { weather: 'sunny', temperature: 75, unit: 'F' },
          latency: 800,
          cost: 0.0027,
          metadata: { model: 'gpt-4', tokens: 450 },
        },
      ],
    },
  },
  {
    id: 'run-2',
    timestamp: '2025-09-30 14:18:32',
    status: 'success',
    totalLatency: 2340,
    totalCost: 0.0089,
    trace: {
      id: 'root-2',
      name: 'Root',
      depth: 0,
      input: { query: 'Analyze this code for bugs', context: { language: 'python' } },
      output: {
        result: 'Found 3 potential issues: memory leak, unused variable, missing error handling',
        suggestions: ['Add try-catch', 'Remove unused var', 'Fix memory leak']
      },
      latency: 2340,
      cost: 0.0089,
      metadata: { model: 'gpt-4', temperature: 0.3 },
      children: [
        {
          id: 'parse-2',
          name: 'ParseCode',
          depth: 1,
          input: { code: 'def process_data():\n    data = load()\n    unused_var = 5\n    result = transform(data)', language: 'python' },
          output: { ast: '...', symbols: ['process_data', 'data', 'unused_var', 'result'] },
          latency: 320,
          cost: 0.0008,
          metadata: { model: 'gpt-3.5-turbo', tokens: 180 },
        },
        {
          id: 'analyze-2',
          name: 'AnalyzeBugs',
          depth: 1,
          input: { ast: '...', symbols: ['process_data', 'data', 'unused_var', 'result'] },
          output: { issues: ['memory_leak', 'unused_variable', 'no_error_handling'] },
          latency: 1120,
          cost: 0.0045,
          metadata: { model: 'gpt-4', tokens: 890 },
          children: [
            {
              id: 'check-memory-2',
              name: 'CheckMemory',
              depth: 2,
              input: { ast: '...', focus: 'memory' },
              output: { found: true, location: 'line 2', severity: 'medium' },
              latency: 450,
              cost: 0.0018,
              metadata: { model: 'gpt-4', tokens: 320 },
            },
            {
              id: 'check-unused-2',
              name: 'CheckUnused',
              depth: 2,
              input: { symbols: ['process_data', 'data', 'unused_var', 'result'] },
              output: { unused: ['unused_var'] },
              latency: 280,
              cost: 0.0011,
              metadata: { model: 'gpt-3.5-turbo', tokens: 150 },
            },
          ],
        },
        {
          id: 'suggest-2',
          name: 'GenSuggestions',
          depth: 1,
          input: { issues: ['memory_leak', 'unused_variable', 'no_error_handling'] },
          output: { suggestions: ['Add try-catch', 'Remove unused var', 'Fix memory leak'] },
          latency: 900,
          cost: 0.0036,
          metadata: { model: 'gpt-4', tokens: 720 },
        },
      ],
    },
  },
  {
    id: 'run-3',
    timestamp: '2025-09-30 13:45:12',
    status: 'error',
    totalLatency: 850,
    totalCost: 0.0021,
    trace: {
      id: 'root-3',
      name: 'RootFunction()',
      depth: 0,
      input: { query: 'Translate to Spanish', text: 'Hello, how are you?' },
      output: { error: 'Translation service timeout' },
      latency: 850,
      cost: 0.0021,
      metadata: { model: 'gpt-4', temperature: 0.5 },
      children: [
        {
          id: 'detect-3',
          name: 'DetectLanguage()',
          depth: 1,
          input: { text: 'Hello, how are you?' },
          output: { language: 'english', confidence: 0.99 },
          latency: 250,
          cost: 0.0008,
          metadata: { model: 'gpt-3.5-turbo', tokens: 45 },
        },
        {
          id: 'translate-3',
          name: 'TranslateText()',
          depth: 1,
          input: { text: 'Hello, how are you?', from: 'english', to: 'spanish' },
          output: { error: 'Timeout after 600ms' },
          latency: 600,
          cost: 0.0013,
          metadata: { model: 'gpt-4', tokens: 0, status: 'timeout' },
        },
      ],
    },
  },
  {
    id: 'run-4',
    timestamp: '2025-09-30 13:22:08',
    status: 'success',
    totalLatency: 3200,
    totalCost: 0.0125,
    trace: {
      id: 'root-4',
      name: 'RootFunction()',
      depth: 0,
      input: {
        query: 'Generate a product recommendation',
        user_profile: { age: 28, interests: ['tech', 'fitness'], purchase_history: ['laptop', 'smartwatch'] }
      },
      output: {
        recommendations: [
          { product: 'Wireless Earbuds', score: 0.92, reason: 'Matches tech and fitness interests' },
          { product: 'Fitness Tracker Pro', score: 0.88, reason: 'Complements smartwatch purchase' },
          { product: 'Laptop Stand', score: 0.75, reason: 'Accessory for recent laptop purchase' }
        ]
      },
      latency: 3200,
      cost: 0.0125,
      metadata: { model: 'gpt-4', temperature: 0.8 },
      children: [
        {
          id: 'analyze-profile-4',
          name: 'AnalyzeUserProfile()',
          depth: 1,
          input: { age: 28, interests: ['tech', 'fitness'], purchase_history: ['laptop', 'smartwatch'] },
          output: {
            segments: ['tech_enthusiast', 'fitness_conscious', 'early_adopter'],
            predicted_budget: 'medium-high'
          },
          latency: 680,
          cost: 0.0028,
          metadata: { model: 'gpt-4', tokens: 520 },
        },
        {
          id: 'fetch-products-4',
          name: 'FetchRelevantProducts()',
          depth: 1,
          input: { segments: ['tech_enthusiast', 'fitness_conscious', 'early_adopter'] },
          output: {
            products: [
              { id: 'p1', name: 'Wireless Earbuds', category: 'tech' },
              { id: 'p2', name: 'Fitness Tracker Pro', category: 'fitness' },
              { id: 'p3', name: 'Laptop Stand', category: 'tech' },
              { id: 'p4', name: 'Smart Water Bottle', category: 'fitness' }
            ]
          },
          latency: 420,
          cost: 0.0015,
          metadata: { model: 'gpt-3.5-turbo', tokens: 280 },
        },
        {
          id: 'score-products-4',
          name: 'ScoreProducts()',
          depth: 1,
          input: {
            products: ['Wireless Earbuds', 'Fitness Tracker Pro', 'Laptop Stand', 'Smart Water Bottle'],
            user_profile: { age: 28, interests: ['tech', 'fitness'] }
          },
          output: {
            scores: [
              { product: 'Wireless Earbuds', score: 0.92 },
              { product: 'Fitness Tracker Pro', score: 0.88 },
              { product: 'Laptop Stand', score: 0.75 },
              { product: 'Smart Water Bottle', score: 0.68 }
            ]
          },
          latency: 1250,
          cost: 0.0052,
          metadata: { model: 'gpt-4', tokens: 980 },
          children: [
            {
              id: 'score-tech-4',
              name: 'ScoreTechProducts()',
              depth: 2,
              input: { products: ['Wireless Earbuds', 'Laptop Stand'] },
              output: { scores: [0.92, 0.75] },
              latency: 580,
              cost: 0.0024,
              metadata: { model: 'gpt-4', tokens: 450 },
            },
            {
              id: 'score-fitness-4',
              name: 'ScoreFitnessProducts()',
              depth: 2,
              input: { products: ['Fitness Tracker Pro', 'Smart Water Bottle'] },
              output: { scores: [0.88, 0.68] },
              latency: 520,
              cost: 0.0021,
              metadata: { model: 'gpt-4', tokens: 380 },
            },
          ],
        },
        {
          id: 'explain-4',
          name: 'GenerateExplanations()',
          depth: 1,
          input: {
            recommendations: ['Wireless Earbuds', 'Fitness Tracker Pro', 'Laptop Stand'],
            user_profile: { interests: ['tech', 'fitness'], purchase_history: ['laptop', 'smartwatch'] }
          },
          output: {
            explanations: [
              'Matches tech and fitness interests',
              'Complements smartwatch purchase',
              'Accessory for recent laptop purchase'
            ]
          },
          latency: 850,
          cost: 0.0030,
          metadata: { model: 'gpt-4', tokens: 680 },
        },
      ],
    },
  },
];

// Custom node component for React Flow
interface TraceNodeData {
  span: TraceSpan;
  isRunning: boolean;
  isCompleted: boolean;
  isDivergent: boolean;
  isOriginalPath: boolean;
  onPlay: (span: TraceSpan, mode: 'single' | 'subtree') => void;
}

const randomSuffix = () => Math.random().toString(36).slice(2, 8);

const NODE_WIDTH = 160;
const NODE_HEIGHT = 64;
const HORIZONTAL_GAP = 120;
const VERTICAL_GAP = 40;

const collectIdsFromSpans = (spans: TraceSpan[]): string[] => {
  const ids: string[] = [];
  spans.forEach((child) => {
    ids.push(child.id);
    if (child.children?.length) {
      ids.push(...collectIdsFromSpans(child.children));
    }
  });
  return ids;
};

const collectBaseSpanIds = (span: TraceSpan): string[] => {
  const ids = [span.id];
  span.children?.forEach((child) => {
    ids.push(...collectBaseSpanIds(child));
  });
  return ids;
};

const generateAltBranch = (parent: TraceSpan): TraceSpan[] => {
  const depthBase = (parent.depth ?? 0) + 1;
  const suffix = randomSuffix();
  const mainId = `${parent.id}-alt-${suffix}`;
  const workerId = `${mainId}-worker`;
  const finalizeId = `${mainId}-finalize`;
  const fallbackId = `${parent.id}-alt-fallback-${suffix}`;

  const mainLatency = 350 + Math.floor(Math.random() * 220);
  const workerLatency = 260 + Math.floor(Math.random() * 160);
  const finalizeLatency = 140 + Math.floor(Math.random() * 120);
  const fallbackLatency = 120 + Math.floor(Math.random() * 100);

  return [
    {
      id: mainId,
      name: 'AltPath1()',
      depth: depthBase,
      input: { reason: 'feature_flag_trigger', parent: parent.name },
      output: { status: 'rerouted', branch: 'alternate-primary' },
      latency: mainLatency,
      cost: parseFloat((0.0015 + Math.random() * 0.001).toFixed(4)),
      metadata: { simulated: true, path: 'alt-primary' },
      children: [
        {
          id: workerId,
          name: 'AltWorker()',
          depth: depthBase + 1,
          input: { payload: 'alternate-data' },
          output: { processed: true, checksum: randomSuffix() },
          latency: workerLatency,
          cost: parseFloat((0.001 + Math.random() * 0.0008).toFixed(4)),
          metadata: { simulated: true, path: 'alt-worker' },
          children: [
            {
              id: finalizeId,
              name: 'AltFinalize()',
              depth: depthBase + 2,
              input: { branch: 'alternate-primary' },
              output: { status: 'complete', result: 'alternate_success' },
              latency: finalizeLatency,
              cost: parseFloat((0.0006 + Math.random() * 0.0005).toFixed(4)),
              metadata: { simulated: true, path: 'alt-finalize' },
            },
          ],
        },
      ],
    },
    {
      id: fallbackId,
      name: 'AltPath2()',
      depth: depthBase,
      input: { reason: 'cache_hit', parent: parent.name },
      output: { status: 'skipped', cached: true },
      latency: fallbackLatency,
      cost: parseFloat((0.0007 + Math.random() * 0.0005).toFixed(4)),
      metadata: { simulated: true, path: 'alt-fallback' },
    },
  ];
};

const TraceNode: React.FC<{ data: TraceNodeData; selected: boolean }> = ({ data, selected }) => {
  const { span, isRunning, isCompleted, isDivergent, isOriginalPath, onPlay } = data;
  const [isHovered, setIsHovered] = useState(false);
  const hasChildren = !!span.children?.length;

  return (
    <div
      className="relative group"
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      <Handle type="target" position={Position.Left} className="!bg-muted-foreground/70 !h-2 !w-2" />
      <div
        className={cn(
          'px-2 py-1 rounded border-2 bg-background shadow-sm transition-all',
          selected && 'border-blue-500 shadow-md',
          !selected && 'border-border',
          isRunning && 'border-blue-400 bg-blue-500/10 animate-pulse',
          isCompleted && 'border-green-400 bg-green-500/10',
          isDivergent && 'border-orange-500 bg-orange-500/20',
          isOriginalPath && 'opacity-45 border-dashed border-muted-foreground/50 bg-muted/40 text-muted-foreground'
        )}
        style={{ width: NODE_WIDTH }}
      >
        <div className="flex items-center gap-1.5">
          <div className="flex-shrink-0">
            <div className={cn('font-mono text-xs font-semibold whitespace-nowrap')}>
              {span.name}
            </div>
            {span.latency && (
              <div className="text-[10px] text-muted-foreground">
                {span.latency}ms
              </div>
            )}
          </div>
          {isHovered && !isRunning && (
            <div className="flex items-center gap-0.5 flex-shrink-0">
              <button
                onClick={() => onPlay(span, 'single')}
                className="p-0.5 rounded hover:bg-accent"
                title="Replay this function only"
              >
                <Play className="size-3 fill-current" />
              </button>
              {hasChildren && (
                <button
                  onClick={() => onPlay(span, 'subtree')}
                  className="p-0.5 rounded hover:bg-accent"
                  title="Replay with all children"
                >
                  <PlayCircle className="size-3" />
                </button>
              )}
            </div>
          )}
        </div>
      </div>
      <Handle type="source" position={Position.Right} className="!bg-muted-foreground/70 !h-2 !w-2" />
    </div>
  );
};

// Helper to build React Flow graph from trace tree using d3-hierarchy layout
const buildFlowGraph = (
  rootSpan: TraceSpan,
  alternateBranches: Record<string, TraceSpan[]>,
  runningSpans: Set<string>,
  completedSpans: Set<string>,
  divergentSpans: Set<string>,
  originalSpans: Set<string>,
  onPlay: (span: TraceSpan, mode: 'single' | 'subtree') => void
): { nodes: Node[]; edges: Edge[] } => {
  const nodes: Node[] = [];
  const edges: Edge[] = [];

  // Convert TraceSpan to d3-hierarchy compatible structure
  const spanToHierarchy = (span: TraceSpan): any => ({
    id: span.id,
    span,
    children: [
      ...(span.children?.map(spanToHierarchy) ?? []),
      ...(alternateBranches[span.id]?.map(spanToHierarchy) ?? []),
    ],
  });

  // Create d3 hierarchy
  const root = hierarchy(spanToHierarchy(rootSpan));

  // Create tree layout with horizontal orientation (left to right)
  const treeLayout = tree<any>()
    .nodeSize([NODE_HEIGHT + VERTICAL_GAP, NODE_WIDTH + HORIZONTAL_GAP])
    .separation((a, b) => (a.parent === b.parent ? 1 : 1.1));

  // Apply layout
  treeLayout(root);

  // Create nodes and edges from layout
  root.descendants().forEach((d: any) => {
    const span = d.data.span;
    const nodeId = span.id;

    // Create React Flow node with d3 computed positions
    nodes.push({
      id: nodeId,
      type: 'traceNode',
      position: {
        x: d.y, // d3 tree uses y for horizontal in vertical layout
        y: d.x  // d3 tree uses x for vertical in vertical layout
      },
      data: {
        span,
        isRunning: runningSpans.has(nodeId),
        isCompleted: completedSpans.has(nodeId),
        isDivergent: divergentSpans.has(nodeId),
        isOriginalPath: originalSpans.has(nodeId),
        onPlay,
      },
      sourcePosition: Position.Right,
      targetPosition: Position.Left,
    });

    // Create edges
    if (d.parent) {
      const parentId = d.parent.data.span.id;
      edges.push({
        id: `${parentId}-${nodeId}`,
        source: parentId,
        target: nodeId,
        type: 'smoothstep',
        animated: runningSpans.has(nodeId),
        style: {
          stroke: divergentSpans.has(nodeId) ? '#f97316' : originalSpans.has(nodeId) ? '#9ca3af' : '#64748b',
          strokeWidth: 2,
          strokeDasharray: originalSpans.has(nodeId) ? '5,5' : undefined,
        },
        markerEnd: {
          type: MarkerType.ArrowClosed,
          color: divergentSpans.has(nodeId) ? '#f97316' : originalSpans.has(nodeId) ? '#9ca3af' : '#64748b',
        },
      });
    }
  });

  return { nodes, edges };
};

interface FunctionDetailsPanelProps {
  span: TraceSpan | null;
}

const FunctionDetailsPanel: React.FC<FunctionDetailsPanelProps> = ({ span }) => {
  if (!span) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
        Select a span to view details
      </div>
    );
  }

  return (
    <div className="h-full overflow-auto p-4 space-y-4">
      <div>
        <h3 className="font-semibold text-lg mb-2">{span.name}</h3>
        <div className="flex gap-4 text-sm text-muted-foreground">
          {span.latency && <span>Latency: {span.latency}ms</span>}
          {span.cost && <span>Cost: ${span.cost.toFixed(4)}</span>}
        </div>
      </div>

      {span.metadata && (
        <div>
          <h4 className="font-medium text-sm mb-2">Metadata</h4>
          <div className="bg-muted/50 rounded p-3 font-mono text-xs">
            <pre className="whitespace-pre-wrap break-words">{JSON.stringify(span.metadata, null, 2)}</pre>
          </div>
        </div>
      )}

      <div>
        <h4 className="font-medium text-sm mb-2">Input</h4>
        <div className="bg-muted/50 rounded p-3 font-mono text-xs overflow-auto max-h-48">
          <pre className="whitespace-pre-wrap break-words">{JSON.stringify(span.input, null, 2)}</pre>
        </div>
      </div>

      <div>
        <h4 className="font-medium text-sm mb-2">Output</h4>
        <div className="bg-muted/50 rounded p-3 font-mono text-xs overflow-auto max-h-48">
          <pre className="whitespace-pre-wrap break-words">{JSON.stringify(span.output, null, 2)}</pre>
        </div>
      </div>
    </div>
  );
};

interface TerminalViewProps {
  logs: string[];
}

const TerminalView: React.FC<TerminalViewProps> = ({ logs }) => {
  const getLogColor = (log: string) => {
    if (log.startsWith('>')) return 'text-blue-400';
    if (log.includes('started')) return 'text-yellow-400';
    if (log.includes('✓') || log.includes('completed')) return 'text-green-400';
    if (log.includes('Input:')) return 'text-cyan-400';
    if (log.includes('Output:')) return 'text-purple-400';
    if (log.includes('error') || log.includes('Error')) return 'text-red-400';
    return 'text-gray-400';
  };

  return (
    <div className="h-full bg-black/95 font-mono text-xs p-4 overflow-auto">
      {logs.map((log, idx) => (
        <div key={idx} className={cn('mb-1 whitespace-pre-wrap break-words', getLogColor(log))}>
          {log}
        </div>
      ))}
    </div>
  );
};

const nodeTypes = {
  traceNode: TraceNode,
};

const WorkflowViewInner: React.FC = () => {
  const [selectedRunIndex, setSelectedRunIndex] = useState(0);
  const [selectedSpan, setSelectedSpan] = useState<TraceSpan | null>(mockTraceRuns[0]?.trace ?? null);
  const [runningSpans, setRunningSpans] = useState<Set<string>>(new Set());
  const [completedSpans, setCompletedSpans] = useState<Set<string>>(new Set());
  const [divergentSpans, setDivergentSpans] = useState<Set<string>>(new Set());
  const [originalSpans, setOriginalSpans] = useState<Set<string>>(new Set());
  const [altBranches, setAltBranches] = useState<Record<string, TraceSpan[]>>({});
  const [logs, setLogs] = useState<string[]>([
    '> Workflow initialized',
    '> Ready to execute trace',
  ]);
  const { fitView } = useReactFlow();

  const currentRun = mockTraceRuns[selectedRunIndex] || mockTraceRuns[0]!;
  const currentTrace = currentRun.trace;

  const findSpanById = useCallback((span: TraceSpan, id: string): TraceSpan | null => {
    if (span.id === id) return span;
    if (span.children) {
      for (const child of span.children) {
        const found = findSpanById(child, id);
        if (found) return found;
      }
    }
    const alternates = altBranches[span.id] ?? [];
    for (const altChild of alternates) {
      const found = findSpanById(altChild, id);
      if (found) return found;
    }
    return null;
  }, [altBranches]);

  const simulateRun = useCallback(async (span: TraceSpan, replayMode: 'single' | 'subtree') => {
    const targetId = span.id;
    const modeLabel = replayMode === 'single' ? '(single)' : '(with children)';
    const hasChildren = !!span.children?.length;
    const shouldAttemptDivergence = replayMode === 'subtree' && hasChildren;
    const willDiverge = shouldAttemptDivergence && Math.random() < 0.5;

    const previousAltNodes = altBranches[targetId] ?? [];
    const previousAltIds = collectIdsFromSpans(previousAltNodes);
    const baseChildIds = hasChildren
      ? span.children!.flatMap((child) => collectBaseSpanIds(child))
      : [];

    const newAltNodes = willDiverge ? generateAltBranch(span) : [];
    const newAltIds = willDiverge ? collectIdsFromSpans(newAltNodes) : [];

    const altLookup = new Map<string, TraceSpan>();
    if (willDiverge) {
      const populateLookup = (nodes: TraceSpan[]) => {
        nodes.forEach((node) => {
          altLookup.set(node.id, node);
          if (node.children?.length) {
            populateLookup(node.children);
          }
        });
      };
      populateLookup(newAltNodes);
    }

    let executionIds: string[] = [];
    if (replayMode === 'single') {
      executionIds = [targetId];
    } else if (willDiverge) {
      executionIds = [targetId, ...newAltIds];
    } else {
      executionIds = collectBaseSpanIds(span);
    }

    setSelectedSpan(span);
    setRunningSpans(new Set());
    setCompletedSpans((prev) => {
      const next = new Set(prev);
      executionIds.forEach((id) => next.delete(id));
      return next;
    });

    setLogs((prev) => {
      const next = [...prev, `\n> Running ${span.name} ${modeLabel}...`];
      if (willDiverge) {
        next.push('⚠️ Alternate path detected – executing divergent branch');
      } else if (shouldAttemptDivergence) {
        next.push('→ Following recorded trace path');
      }
      return next;
    });

    if (willDiverge) {
      setAltBranches((prev) => ({ ...prev, [targetId]: newAltNodes }));
      setOriginalSpans((prev) => {
        const next = new Set(prev);
        baseChildIds.forEach((id) => next.add(id));
        return next;
      });
      setDivergentSpans((prev) => {
        const next = new Set(prev);
        previousAltIds.forEach((id) => next.delete(id));
        newAltIds.forEach((id) => next.add(id));
        return next;
      });
    } else {
      if (previousAltNodes.length) {
        setAltBranches((prev) => {
          if (!prev[targetId]) return prev;
          const next = { ...prev };
          delete next[targetId];
          return next;
        });
      }
      if (baseChildIds.length) {
        setOriginalSpans((prev) => {
          const next = new Set(prev);
          baseChildIds.forEach((id) => next.delete(id));
          return next;
        });
      }
      if (previousAltIds.length) {
        setDivergentSpans((prev) => {
          const next = new Set(prev);
          previousAltIds.forEach((id) => next.delete(id));
          return next;
        });
      }
    }

    for (const spanId of executionIds) {
      setRunningSpans((prev) => new Set([...prev, spanId]));

      const currentSpan = findSpanById(currentTrace, spanId) ?? altLookup.get(spanId) ?? null;
      if (currentSpan) {
        setLogs((prev) => [
          ...prev,
          `  → ${currentSpan.name} started`,
          `  Input: ${JSON.stringify(currentSpan.input, null, 2)}`,
        ]);
      }

      await new Promise((resolve) => setTimeout(resolve, 800));

      setRunningSpans((prev) => {
        const next = new Set(prev);
        next.delete(spanId);
        return next;
      });
      setCompletedSpans((prev) => new Set([...prev, spanId]));

      if (currentSpan) {
        setLogs((prev) => [
          ...prev,
          `  Output: ${JSON.stringify(currentSpan.output, null, 2)}`,
          `  ✓ ${currentSpan.name} completed in ${currentSpan.latency ?? '—'}ms`,
        ]);
      }
    }

    if (willDiverge) {
      const altNames = newAltNodes.map((node) => node.name).join(', ');
      setLogs((prev) => [
        ...prev,
        `  → Divergent branch executed (${altNames || 'alternate nodes'})`,
        `> ${span.name} execution complete ${modeLabel}\n`,
      ]);
    } else if (replayMode === 'single') {
      setLogs((prev) => [
        ...prev,
        `> ${span.name} execution complete ${modeLabel}`,
        '✓ Single function replay complete',
      ]);
    } else {
      setLogs((prev) => [
        ...prev,
        `> ${span.name} execution complete ${modeLabel}`,
        '✓ Execution followed recorded trace',
      ]);
    }
  }, [altBranches, currentTrace, findSpanById]);

  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  // Update nodes/edges when dependencies change using d3-hierarchy layout
  useEffect(() => {
    const { nodes: newNodes, edges: newEdges } = buildFlowGraph(
      currentTrace,
      altBranches,
      runningSpans,
      completedSpans,
      divergentSpans,
      originalSpans,
      simulateRun
    );
    console.log('Setting nodes:', newNodes.length, 'edges:', newEdges.length);
    setNodes(newNodes);
    setEdges(newEdges);
  }, [currentTrace, altBranches, runningSpans, completedSpans, divergentSpans, originalSpans, simulateRun, setNodes, setEdges]);

  useEffect(() => {
    if (!nodes.length) return;
    requestAnimationFrame(() => {
      fitView({ padding: 0.24, duration: 400 });
    });
  }, [fitView, nodes, edges]);

  const onNodeClick = useCallback((_event: React.MouseEvent, node: Node) => {
    const span = node.data.span as TraceSpan;
    setSelectedSpan(span);
  }, []);

  if (!mockTraceRuns || mockTraceRuns.length === 0) {
    return <div className="p-4 text-red-500">No mock trace runs available</div>;
  }

  return (
    <div className="flex h-full min-h-0 w-full flex-col overflow-hidden rounded-md border bg-background">
      <ResizablePanelGroup direction="vertical" className="flex-1 min-h-0">
        <ResizablePanel defaultSize={60} minSize={30}>
          <ResizablePanelGroup direction="horizontal">
            <ResizablePanel defaultSize={50} minSize={30}>
              <div className="h-full min-h-0 flex flex-col">
                <div className="px-4 py-2 border-b space-y-2">
                  <h3 className="font-semibold text-sm">Trace History</h3>
                  <div className="flex gap-1 overflow-x-auto pb-1">
                    {mockTraceRuns.map((run, idx) => (
                      <button
                        key={run.id}
                        onClick={() => {
                          setSelectedRunIndex(idx);
                          setSelectedSpan(run.trace);
                          setRunningSpans(new Set());
                          setCompletedSpans(new Set());
                          setDivergentSpans(new Set());
                          setOriginalSpans(new Set());
                          setAltBranches({});
                          setLogs([`> Loaded trace from ${run.timestamp}`, '> Ready to execute trace']);
                        }}
                        className={cn(
                          'px-3 py-1.5 rounded text-xs font-mono whitespace-nowrap transition-colors border',
                          selectedRunIndex === idx
                            ? 'bg-accent border-accent-foreground/20'
                            : 'bg-muted/50 hover:bg-muted border-transparent',
                          run.status === 'error' && 'text-red-500'
                        )}
                      >
                        <div className="flex items-center gap-1.5">
                          <span className={cn(
                            'size-1.5 rounded-full',
                            run.status === 'success' && 'bg-green-500',
                            run.status === 'error' && 'bg-red-500',
                            run.status === 'partial' && 'bg-yellow-500'
                          )} />
                          <span>{run.timestamp}</span>
                          <span className="text-muted-foreground">•</span>
                          <span>{run.totalLatency}ms</span>
                          <span className="text-muted-foreground">•</span>
                          <span>${run.totalCost.toFixed(4)}</span>
                        </div>
                      </button>
                    ))}
                  </div>
                </div>
                <div className="flex-1 min-h-0">
                  <ReactFlow
                    nodes={nodes}
                    edges={edges}
                    onNodesChange={onNodesChange}
                    onEdgesChange={onEdgesChange}
                    onNodeClick={onNodeClick}
                    nodeTypes={nodeTypes}
                    nodeOrigin={[0.5, 0.5]}
                    fitView
                    fitViewOptions={{ padding: 0.24 }}
                    minZoom={0.5}
                    maxZoom={1.5}
                    panOnScroll
                    selectionOnDrag
                    panOnDrag={false}
                    defaultEdgeOptions={{
                      type: 'smoothstep',
                    }}
                  />
                </div>
              </div>
            </ResizablePanel>

            <ResizableHandle withHandle />

            <ResizablePanel defaultSize={50} minSize={30}>
              <div className="h-full min-h-0 flex flex-col border-l">
                <div className="px-4 py-2 border-b">
                  <h3 className="font-semibold text-sm">Function Details</h3>
                </div>
                <div className="flex-1 overflow-auto">
                  <FunctionDetailsPanel span={selectedSpan} />
                </div>
              </div>
            </ResizablePanel>
          </ResizablePanelGroup>
        </ResizablePanel>

        <ResizableHandle withHandle />

        <ResizablePanel defaultSize={40} minSize={20}>
          <div className="h-full min-h-0 flex flex-col">
            <div className="px-4 py-2 border-t bg-muted/30">
              <h3 className="font-semibold text-sm">Terminal</h3>
            </div>
            <div className="flex-1 overflow-auto">
              <TerminalView logs={logs} />
            </div>
          </div>
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
};

export const WorkflowView: React.FC = () => (
  <ReactFlowProvider>
    <WorkflowViewInner />
  </ReactFlowProvider>
);
