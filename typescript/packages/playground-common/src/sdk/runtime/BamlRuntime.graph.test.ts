import { describe, it, expect } from 'vitest';
import { WasmControlFlowNodeType } from '@gloo-ai/baml-schema-wasm-web/baml_schema_build';
import type { WasmControlFlowGraph, WasmSpan } from '@gloo-ai/baml-schema-wasm-web/baml_schema_build';
import type { WasmTypeAdapter } from '../interface';
import {
  buildControlFlowArtifacts,
  createFallbackControlFlowArtifacts,
  type ControlFlowOptions,
} from './BamlRuntime';

const mockAdapter: Pick<WasmTypeAdapter, 'convertSpan'> = {
  convertSpan: (span: any) => ({
    filePath: span.file_path,
    start: span.start,
    end: span.end,
    startLine: span.start_line,
    startColumn: span.start_column,
    endLine: span.end_line,
    endColumn: span.end_column,
  }),
};

const span = (start: number, end: number): WasmSpan => (
  {
    file_path: 'baml_src/workflows/simple.baml',
    start,
    end,
    start_line: 0,
    start_column: 0,
    end_line: 0,
    end_column: 0,
  } as unknown as WasmSpan
);

const workflowGraph = {
  nodes: [
    {
      id: 0,
      parent_id: undefined,
      lexical_id: 'SimpleWorkflow|root:0',
      label: 'SimpleWorkflow',
      span: span(0, 293),
      node_type: WasmControlFlowNodeType.FunctionRoot,
    },
    {
      id: 1,
      parent_id: 0,
      lexical_id: 'SimpleWorkflow|root:0|hdr:gather-applicant-context:0',
      label: 'gather applicant context',
      span: span(55, 83),
      node_type: WasmControlFlowNodeType.HeaderContextEnter,
    },
    {
      id: 2,
      parent_id: 0,
      lexical_id: 'SimpleWorkflow|root:0|hdr:normalize-profile-signals:1',
      label: 'normalize profile signals',
      span: span(123, 152),
      node_type: WasmControlFlowNodeType.HeaderContextEnter,
    },
  ],
  edges: [
    { src: 1, dst: 2 },
  ],
} as unknown as WasmControlFlowGraph;

const opts: ControlFlowOptions = {
  rootName: 'SimpleWorkflow',
  rootType: 'workflow',
  llmClient: undefined,
  timestamp: 123,
};

const conditionalGraph = {
  nodes: [
    {
      id: 0,
      parent_id: undefined,
      lexical_id: 'ConditionalWorkflow|root:0',
      label: 'ConditionalWorkflow',
      span: span(0, 500),
      node_type: WasmControlFlowNodeType.FunctionRoot,
    },
    {
      id: 1,
      parent_id: 0,
      lexical_id: 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:0',
      label: 'check summary confidence',
      span: span(100, 150),
      node_type: WasmControlFlowNodeType.HeaderContextEnter,
    },
    {
      id: 2,
      parent_id: 1,
      lexical_id: 'ConditionalWorkflow|root:0|hdr:check-summary-confidence:0|bg:if-guard:0',
      label: 'if (guard)',
      span: span(160, 400),
      node_type: WasmControlFlowNodeType.BranchGroup,
    },
    {
      id: 3,
      parent_id: 2,
      lexical_id:
        'ConditionalWorkflow|root:0|hdr:check-summary-confidence:0|bg:if-guard:0|arm:true:0|hdr:run-enrichment:0',
      label: 'run enrichment',
      span: span(200, 250),
      node_type: WasmControlFlowNodeType.HeaderContextEnter,
    },
    {
      id: 4,
      parent_id: 2,
      lexical_id:
        'ConditionalWorkflow|root:0|hdr:check-summary-confidence:0|bg:if-guard:0|arm:false:1|hdr:return-guidance:0',
      label: 'return guidance',
      span: span(300, 350),
      node_type: WasmControlFlowNodeType.HeaderContextEnter,
    },
  ],
  edges: [
    { src: 1, dst: 2 },
    { src: 2, dst: 3 },
    { src: 2, dst: 4 },
  ],
} as unknown as WasmControlFlowGraph;

describe('buildControlFlowArtifacts', () => {
  it('converts workflow graphs with stable ids and edges', () => {
    const result = buildControlFlowArtifacts(
      workflowGraph,
      mockAdapter as WasmTypeAdapter,
      opts,
    );

    expect(result).toBeTruthy();
    // Root node now uses lexical_id format with |root:0
    expect(result?.callGraph.id).toBe('SimpleWorkflow|root:0');
    expect(result?.callGraph.type).toBe('block');

    const rootNode = result?.nodes[0];
    expect(rootNode?.id).toBe('SimpleWorkflow|root:0');
    expect(rootNode?.type).toBe('group');

    const child = result?.nodes.find((n) => n.id.includes('gather-applicant-context'));
    expect(child?.parent).toBe('SimpleWorkflow|root:0');

    expect(result?.edges[0]).toMatchObject({
      source: 'SimpleWorkflow|root:0|hdr:gather-applicant-context:0',
      target: 'SimpleWorkflow|root:0|hdr:normalize-profile-signals:1',
    });
  });

  it('creates fallback graphs with group roots for workflow metadata', () => {
    const metadata = {
      name: 'SimpleWorkflow',
      type: 'workflow' as const,
      functionFlavor: 'llm' as const,
      span: mockAdapter.convertSpan(span(0, 10)),
      signature: '',
      testSnippet: '',
      testCases: [],
      clientName: undefined,
      orchestrationGraph: undefined,
    };

    const fallback = createFallbackControlFlowArtifacts(metadata, 1000);
    expect(fallback.nodes[0]?.type).toBe('group');
    expect(fallback.callGraph.type).toBe('block');
  });

  it('omits edges that only restate parent-child relationships', () => {
    const result = buildControlFlowArtifacts(
      conditionalGraph,
      mockAdapter as WasmTypeAdapter,
      {
        ...opts,
        rootName: 'ConditionalWorkflow',
      },
    );

    expect(result).toBeTruthy();
    const edgeSources = result?.edges.map((edge) => edge.source) ?? [];
    // Edge from header (id:1) to branch group (id:2) remains
    expect(edgeSources).toContain(
      'ConditionalWorkflow|root:0|hdr:check-summary-confidence:0',
    );
    // Edges from branch group to its arm headers should be removed because parent already conveys nesting
    expect(
      edgeSources.some((src) => src.includes('|bg:if-guard:0')),
    ).toBe(false);
    const runEnrichmentNode = result?.nodes.find((n) => n.label === 'run enrichment');
    expect(runEnrichmentNode?.parent).toBe(
      'ConditionalWorkflow|root:0|hdr:check-summary-confidence:0|bg:if-guard:0',
    );
  });
});
