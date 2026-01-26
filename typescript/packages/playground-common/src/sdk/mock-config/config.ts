/**
 * Centralized Mock Data Configuration
 *
 * All mock workflows, test cases, and output generators in one place
 */

import type { GraphNode, GraphEdge, TestCaseInput, BAMLFile } from '../types';
import type { MockRuntimeConfig, NodeOutputGenerator } from './types';
import {
  createMockFunction as createMockFunctionUnified,
  createMockTestCase as createMockTestCaseUnified,
  createMockSpan,
  type FunctionMetadata,
  type FunctionWithCallGraph,
  type TestCaseMetadata,
  type PromptInfo,
} from '../interface';

/**
 * Helper to create a workflow
 */
function createWorkflow(
  id: string,
  nodes: Array<{
    id: string;
    label: string;
    type?: GraphNode['type'];
    parent?: string;
    llmClient?: string;
  }>,
  edges: Array<{ from: string; to: string; label?: string }>,
  options?: {
    parameters?: Array<{ name: string; type: string; optional: boolean }>;
    returnType?: string;
    filePath?: string; // Allow explicit file path specification
  }
): FunctionWithCallGraph {
  const graphNodes: GraphNode[] = [
    // Add the workflow itself as the first node (entry point)
    {
      id,
      type: 'function',
      label: id,
      functionName: id,
      codeHash: `hash_${id}_${Date.now()}`,
      lastModified: Date.now(),
    },
    ...nodes.map((n) => ({
      id: n.id,
      type: n.type || 'function',
      label: n.label,
      functionName: n.id,
      // Only set parent for explicitly defined parent-child relationships (like subgraphs)
      parent: n.parent,
      llmClient: n.llmClient,
      codeHash: `hash_${n.id}_${Date.now()}`,
      lastModified: Date.now(),
    })),
  ];

  const graphEdges: GraphEdge[] = edges.map((e, idx) => ({
    id: `edge_${idx}`,
    source: e.from,
    target: e.to,
    label: e.label,
  }));

  const filePath = options?.filePath || `/mock/${id}.baml`;
  const displayName = id.replace(/([A-Z])/g, ' $1').trim();

  return {
    // FunctionMetadata fields
    name: id,
    type: 'workflow',
    functionFlavor: 'llm',
    span: createMockSpan(filePath),
    signature: `function ${id}(...)`,
    testSnippet: '',
    testCases: [],

    // Call graph fields
    callGraph: {
      id,
      type: 'block',
      children: nodes.map((n) => ({
        id: n.id,
        type: n.type === 'llm_function' ? 'llm_function' : 'function',
        children: [],
      })),
    },
    isRoot: true,
    callGraphDepth: 1,

    // Workflow compatibility fields (id, displayName, nodes, edges, etc.)
    id,
    displayName,
    filePath,
    startLine: 1,
    endLine: 100,
    nodes: graphNodes,
    edges: graphEdges,
    entryPoint: id,
    parameters: options?.parameters || [
      { name: 'input', type: 'any', optional: false },
    ],
    returnType: options?.returnType || 'any',
    childFunctions: [id, ...nodes.map((n) => n.id)],
    lastModified: Date.now(),
    codeHash: `hash_${id}`,
  };
}

/**
 * Create mock workflows
 */
function createMockWorkflows(): FunctionWithCallGraph[] {
  return [
    // 1. Simple Linear Workflow
    createWorkflow(
      'simpleWorkflow',
      [
        { id: 'fetchData', label: 'Fetch Data', type: 'function' },
        {
          id: 'processData',
          label: 'Process Data',
          type: 'llm_function',
          llmClient: 'GPT-4o',
        },
        { id: 'saveResult', label: 'Save Result', type: 'function' },
      ],
      [
        { from: 'simpleWorkflow', to: 'fetchData' },
        { from: 'fetchData', to: 'processData' },
        { from: 'processData', to: 'saveResult' },
      ],
      {
        parameters: [{ name: 'input', type: 'string', optional: false }],
        returnType: '{ result: string; processed: boolean }',
        filePath: 'workflows/simple.baml',
      }
    ),

    // 2. Conditional Workflow with Subgraph
    createWorkflow(
      'conditionalWorkflow',
      [
        { id: 'validateInput', label: 'Validate Input', type: 'function' },
        { id: 'checkCondition', label: 'Check Condition', type: 'conditional' },
        {
          id: 'handleSuccess',
          label: 'Analyze Result',
          type: 'llm_function',
          llmClient: 'Claude-3',
        },
        { id: 'handleFailure', label: 'Handle Failure', type: 'function' },

        // Subgraph: Nested processing (group node)
        { id: 'PROCESSING_SUBGRAPH', label: 'ProcessingWorkflow', type: 'group' },
        {
          id: 'subgraph_process',
          label: 'Process Data',
          type: 'function',
          parent: 'PROCESSING_SUBGRAPH',
        },
        {
          id: 'subgraph_validate',
          label: 'Validate Result',
          type: 'function',
          parent: 'PROCESSING_SUBGRAPH',
        },
      ],
      [
        { from: 'conditionalWorkflow', to: 'validateInput' },
        { from: 'validateInput', to: 'checkCondition' },
        { from: 'checkCondition', to: 'handleSuccess', label: 'success' },
        { from: 'checkCondition', to: 'handleFailure', label: 'failure' },
        { from: 'handleSuccess', to: 'subgraph_process' },
        { from: 'subgraph_process', to: 'subgraph_validate' },
      ],
      {
        parameters: [
          { name: 'data', type: 'any', optional: false },
          { name: 'threshold', type: 'number', optional: true },
        ],
        returnType: '{ success: boolean; result?: any }',
        filePath: 'workflows/conditional.baml',
      }
    ),

    // 3. Shared Workflow
    createWorkflow(
      'sharedWorkflow',
      [
        { id: 'aggregateData', label: 'Aggregate Data', type: 'function' },
        { id: 'fetchData', label: 'Fetch Data', type: 'function' },
      ],
      [
        { from: 'sharedWorkflow', to: 'aggregateData' },
        { from: 'aggregateData', to: 'fetchData' },
      ],
      {
        parameters: [{ name: 'sources', type: 'string[]', optional: false }],
        returnType: '{ aggregated: any; count: number }',
        filePath: 'workflows/shared.baml',
      }
    ),
  ];
}

/**
 * Create mock test cases
 */
function createMockTestCases(): Record<string, TestCaseMetadata[]> {
  const mockSpan = createMockSpan('tests/mock.test.ts');

  return {
    simpleWorkflow: [
      {
        id: 'test_simpleWorkflow_one',
        name: 'test_one',
        source: 'test' as const,
        functionId: 'simpleWorkflow',
        filePath: 'tests/simpleWorkflow.test.ts',
        inputs: [
          { name: 'input', value: 'test data' },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'simpleWorkflow', start: 0, end: 100 }],
      },
      {
        id: 'test_simpleWorkflow_two',
        name: 'test_two',
        source: 'test' as const,
        functionId: 'simpleWorkflow',
        filePath: 'tests/simpleWorkflow.test.ts',
        inputs: [
          { name: 'input', value: 'different test data' },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'simpleWorkflow', start: 0, end: 100 }],
      },
    ],
    conditionalWorkflow: [
      {
        id: 'test_conditionalWorkflow_success',
        name: 'success_path',
        source: 'test' as const,
        functionId: 'conditionalWorkflow',
        filePath: 'tests/conditionalWorkflow.test.ts',
        inputs: [
          { name: 'data', value: JSON.stringify({ valid: true }) },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'conditionalWorkflow', start: 0, end: 100 }],
      },
    ],
    fetchData: [
      {
        id: 'test_fetchData_success',
        name: 'test_fetchData_success',
        source: 'test' as const,
        functionId: 'fetchData',
        filePath: 'tests/fetchData.test.ts',
        inputs: [
          { name: 'url', value: 'https://api.example.com/data' },
          { name: 'timeout', value: '5000' },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'fetchData', start: 0, end: 100 }],
      },
      {
        id: 'test_fetchData_timeout',
        name: 'test_fetchData_timeout',
        source: 'test' as const,
        functionId: 'fetchData',
        filePath: 'tests/fetchData.test.ts',
        inputs: [
          { name: 'url', value: 'https://slow-api.example.com/data' },
          { name: 'timeout', value: '100' },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'fetchData', start: 0, end: 100 }],
      },
    ],
    processData: [
      {
        id: 'test_processData_valid',
        name: 'test_processData_valid',
        source: 'test' as const,
        functionId: 'processData',
        filePath: 'tests/processData.test.ts',
        inputs: [
          { name: 'data', value: JSON.stringify({ id: 1, value: 'test' }) },
          { name: 'format', value: 'json' },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'processData', start: 0, end: 100 }],
      },
      {
        id: 'test_processData_empty',
        name: 'test_processData_empty',
        source: 'test' as const,
        functionId: 'processData',
        filePath: 'tests/processData.test.ts',
        inputs: [
          { name: 'data', value: JSON.stringify({}) },
          { name: 'format', value: 'json' },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'processData', start: 0, end: 100 }],
      },
    ],
    validateInput: [
      {
        id: 'test_validateInput_valid_email',
        name: 'test_validateInput_valid_email',
        source: 'test' as const,
        functionId: 'validateInput',
        filePath: 'tests/validateInput.test.ts',
        inputs: [
          { name: 'data', value: JSON.stringify({ email: 'test@example.com', age: 25 }) },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'validateInput', start: 0, end: 100 }],
      },
      {
        id: 'test_validateInput_invalid_email',
        name: 'test_validateInput_invalid_email',
        source: 'test' as const,
        functionId: 'validateInput',
        filePath: 'tests/validateInput.test.ts',
        inputs: [
          { name: 'data', value: JSON.stringify({ email: 'not-an-email', age: 25 }) },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'validateInput', start: 0, end: 100 }],
      },
    ],
    handleSuccess: [
      {
        id: 'test_handleSuccess_normal',
        name: 'test_handleSuccess_normal',
        source: 'test' as const,
        functionId: 'handleSuccess',
        filePath: 'tests/handleSuccess.test.ts',
        inputs: [
          { name: 'result', value: JSON.stringify({ status: 'valid' }) },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'handleSuccess', start: 0, end: 100 }],
      },
    ],
    subgraph_process: [
      {
        id: 'test_subgraph_process_data',
        name: 'process_data',
        source: 'test' as const,
        functionId: 'subgraph_process',
        filePath: 'tests/subgraph.test.ts',
        inputs: [
          { name: 'data', value: 'test data' },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'subgraph_process', start: 0, end: 100 }],
      },
    ],
    aggregateData: [
      {
        id: 'test_aggregateData_multiple',
        name: 'test_aggregateData_multiple',
        source: 'test' as const,
        functionId: 'aggregateData',
        filePath: 'tests/aggregateData.test.ts',
        inputs: [
          { name: 'sources', value: JSON.stringify(['api1', 'api2', 'api3']) },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'aggregateData', start: 0, end: 100 }],
      },
    ],
    standaloneLlmFunction: [
      {
        id: 'test_standalone_llm_function',
        name: 'test_standalone_llm_function',
        source: 'test' as const,
        functionId: 'standaloneLlmFunction',
        filePath: 'tests/standaloneLlmFunction.test.ts',
        inputs: [
          { name: 'prompt', value: 'Say hello to the user.' },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'standaloneLlmFunction', start: 0, end: 100 }],
      },
    ],
    extractUser: [
      {
        id: 'test_extract_valid_user',
        name: 'test_extract_valid_user',
        source: 'test' as const,
        functionId: 'extractUser',
        filePath: 'tests/extractUser.test.ts',
        inputs: [
          { name: 'text', value: 'John Doe, age 30, email: john@example.com' },
        ],
        span: mockSpan,
        parentFunctions: [{ name: 'extractUser', start: 0, end: 100 }],
      },
    ],
  };
}

/**
 * Create mock output generators
 */
function createOutputGenerators(): Record<string, NodeOutputGenerator> {
  return {
    // Workflow entry points
    simpleWorkflow: (ctx) => ({
      initialized: true,
      input: ctx.input || 'default',
      workflowStarted: Date.now(),
    }),

    conditionalWorkflow: (ctx) => ({
      initialized: true,
      data: ctx.data || {},
      threshold: ctx.threshold || 0.5,
      workflowStarted: Date.now(),
    }),

    // Simple workflow outputs
    fetchData: (ctx, inputs) => ({
      data: {
        id: Math.floor(Math.random() * 1000),
        value: (ctx.input as any) || 'sample data',
        timestamp: Date.now(),
      },
      records: Math.floor(Math.random() * 100) + 10,
    }),

    processData: (ctx) => ({
      result: `Processed: ${(ctx.data as any)?.value || 'data'}`,
      tokens: Math.floor(Math.random() * 500) + 200,
      model: 'gpt-4',
      confidence: (Math.random() * 0.3 + 0.7).toFixed(2),
    }),

    saveResult: () => ({
      saved: true,
      id: `RESULT-${Date.now()}`,
      location: '/storage/results',
    }),

    // Conditional workflow outputs
    validateInput: (ctx) => ({
      valid: true,
      data: ctx.data || { value: 'validated' },
      checks: ['format', 'schema', 'range'],
    }),

    checkCondition: () => ({
      condition: Math.random() > 0.4 ? 'success' : 'failure',
      passed: Math.random() > 0.4,
      score: Math.random().toFixed(2),
    }),

    handleSuccess: () => ({
      status: 'success',
      message: 'Condition met, proceeding to subgraph',
      nextStep: 'subgraph',
    }),

    handleFailure: () => ({
      status: 'failure',
      message: 'Condition not met, skipping subgraph',
      reason: 'Below threshold',
    }),

    subgraph_process: () => ({
      subgraphResult: 'processed in subgraph',
      operations: ['transform', 'validate', 'enrich'],
      processingTime: Math.floor(Math.random() * 500) + 100,
    }),

    subgraph_validate: (ctx) => ({
      validated: true,
      subgraphComplete: true,
      finalOutput: {
        ...ctx,
        subgraphProcessed: true,
      },
    }),

    // Shared workflow outputs
    aggregateData: (ctx) => {
      const sources = (ctx.sources as string[]) || ['default'];
      return {
        aggregated: {
          sources: sources,
          total: sources.length,
          timestamp: Date.now(),
        },
        count: sources.length,
        dataSize: Math.floor(Math.random() * 5000) + 1000,
      };
    },

    // Standalone function outputs
    extractUser: (ctx) => ({
      name: 'John Doe',
      age: 30,
      email: 'john@example.com',
      confidence: 0.95,
      extracted: true,
    }),
  };
}

/**
 * Create a mock function with call graph
 * Now uses unified type system
 */
function createMockFunction(
  name: string,
  type: 'function' | 'llm_function' | 'block',
  filePath: string,
  testCases: TestCaseMetadata[] = []
): FunctionWithCallGraph {
  // Convert block to workflow for the base function generator
  const functionType = type === 'block' ? 'workflow' : type;
  const baseFunction = createMockFunctionUnified(name, functionType, filePath, {
    testCases,
    flavor: type === 'llm_function' || type === 'block' ? 'llm' : 'expr',
  });

  // Call graph type should match the input type
  const callGraphType = type;

  // Add call graph and workflow compatibility fields
  return {
    ...baseFunction,
    // Workflow compatibility (FunctionWithCallGraph extends FunctionMetadata with workflow fields)
    id: name,
    displayName: name,
    filePath: baseFunction.span.filePath,
    startLine: baseFunction.span.startLine,
    endLine: baseFunction.span.endLine,
    nodes: [{
      id: name,
      type: type === 'llm_function' ? 'llm_function' : 'function',
      label: name,
      functionName: name,
      codeHash: '',
      lastModified: Date.now(),
    }],
    edges: [],
    entryPoint: name,
    parameters: [],
    returnType: '',
    childFunctions: [],
    lastModified: Date.now(),
    codeHash: '',
    // Call graph fields
    callGraph: {
      id: name,
      type: callGraphType,
      children: [],
    },
    isRoot: true,
    callGraphDepth: 1,
  };
}

/**
 * Create a mock TestCaseMetadata
 * Now uses unified type system
 */
function createMockTestCase(
  name: string,
  parentFunctionName: string,
  filePath: string
): TestCaseMetadata {
  // Use unified mock generator
  return createMockTestCaseUnified(name, parentFunctionName, filePath);
}

/**
 * Create mock BAML files
 */
function createBAMLFiles(
  testCases: Record<string, TestCaseMetadata[]> = createMockTestCases()
): BAMLFile[] {
  // Keep a single source of truth for testCases so validations can compare data structures.
  const localTestCases = testCases;

  return [
    {
      path: 'workflows/simple.baml',
      functions: [
        createMockFunction('simpleWorkflow', 'block', 'workflows/simple.baml', localTestCases['simpleWorkflow'] || []),
        createMockFunction('fetchData', 'function', 'workflows/simple.baml', localTestCases['fetchData'] || []),
        createMockFunction('processData', 'llm_function', 'workflows/simple.baml', localTestCases['processData'] || []),
        createMockFunction('saveResult', 'function', 'workflows/simple.baml'),
      ],
      tests: [
        // Function-level tests (matching old app structure)
        { name: 'test_fetchData_success', functionName: 'fetchData', filePath: 'workflows/simple.baml', nodeType: 'function' as const },
        { name: 'test_fetchData_timeout', functionName: 'fetchData', filePath: 'workflows/simple.baml', nodeType: 'function' as const },
        { name: 'test_processData_valid', functionName: 'processData', filePath: 'workflows/simple.baml', nodeType: 'llm_function' as const },
        { name: 'test_processData_empty', functionName: 'processData', filePath: 'workflows/simple.baml', nodeType: 'llm_function' as const },
      ],
    },
    {
      path: 'workflows/conditional.baml',
      functions: [
        createMockFunction('conditionalWorkflow', 'block', 'workflows/conditional.baml', localTestCases['conditionalWorkflow'] || []),
        createMockFunction('validateInput', 'function', 'workflows/conditional.baml', localTestCases['validateInput'] || []),
        // Note: checkCondition is NOT included as a clickable function (it's a conditional node in the graph)
        // This matches the old baml-graph app behavior where conditional nodes are not directly clickable
        createMockFunction('handleSuccess', 'llm_function', 'workflows/conditional.baml', localTestCases['handleSuccess'] || []),
        createMockFunction('handleFailure', 'function', 'workflows/conditional.baml'),
        // Note: subgraph nodes are NOT included as clickable functions
        // They are internal to the PROCESSING_SUBGRAPH group node
      ],
      tests: [
        // Function-level tests (matching old app structure)
        { name: 'test_validateInput_valid_email', functionName: 'validateInput', filePath: 'workflows/conditional.baml', nodeType: 'function' as const },
        { name: 'test_validateInput_invalid_email', functionName: 'validateInput', filePath: 'workflows/conditional.baml', nodeType: 'function' as const },
        { name: 'test_handleSuccess_normal', functionName: 'handleSuccess', filePath: 'workflows/conditional.baml', nodeType: 'llm_function' as const },
      ],
    },
    {
      path: 'workflows/shared.baml',
      functions: [
        createMockFunction('sharedWorkflow', 'block', 'workflows/shared.baml'),
        createMockFunction('aggregateData', 'function', 'workflows/shared.baml', localTestCases['aggregateData'] || []),
      ],
      tests: [
        { name: 'test_aggregateData_multiple', functionName: 'aggregateData', filePath: 'workflows/shared.baml', nodeType: 'function' as const },
      ],
    },
    {
      path: 'functions/utils.baml',
      functions: [
        createMockFunction('extractUser', 'llm_function', 'functions/utils.baml', localTestCases['extractUser'] || []),
        createMockFunction(
          'standaloneLlmFunction',
          'llm_function',
          'functions/utils.baml',
          localTestCases['standaloneLlmFunction'] || []
        ),
        createMockFunction('helperFunction', 'function', 'functions/utils.baml'),
      ],
      tests: [
        { name: 'test_extract_valid_user', functionName: 'extractUser', filePath: 'functions/utils.baml', nodeType: 'llm_function' as const },
        {
          name: 'test_standalone_llm_function',
          functionName: 'standaloneLlmFunction',
          filePath: 'functions/utils.baml',
          nodeType: 'llm_function' as const,
        },
      ],
    },
  ];
}

function createFunctionPrompts(): Record<
  string,
  { prompt: PromptInfo; curl: { withSecrets: string; withoutSecrets: string } }
> {
  const makeChatPrompt = (
    clientName: string,
    system: string,
    userExample: string
  ): PromptInfo => ({
    type: 'chat',
    clientName,
    messages: [
      {
        role: 'system',
        parts: [{ type: 'text' as const, content: system }],
      },
      {
        role: 'user',
        parts: [{ type: 'text' as const, content: userExample }],
      },
    ],
  });

  const makeCurl = (
    model: string,
    prompt: string
  ) => ({
    withoutSecrets: `curl https://api.openai.com/v1/chat/completions \\\n  -H "Authorization: Bearer ****" \\\n  -H "Content-Type: application/json" \\\n  -d '{"model":"${model}","messages":[{"role":"system","content":"${prompt.replace(/"/g, '\\"')}"}]}'`,
    withSecrets: `curl https://api.openai.com/v1/chat/completions \\\n  -H "Authorization: Bearer sk-mock" \\\n  -H "Content-Type: application/json" \\\n  -d '{"model":"${model}","messages":[{"role":"system","content":"${prompt.replace(/"/g, '\\"')}"}]}'`,
  });

  return {
    processData: {
      prompt: makeChatPrompt(
        'GPT-4o',
        'You are a data normalization agent. Clean up and summarize incoming JSON payloads.',
        'Dataset: {"orders": [...]}\nThreshold: 0.8'
      ),
      curl: makeCurl('gpt-4o-mini', 'Normalize the provided dataset and call out anomalies.'),
    },
    handleSuccess: {
      prompt: makeChatPrompt(
        'Claude-3',
        'You turn successful workflow outputs into short human-readable blurbs.',
        'Success payload: {"result":"widgets created","confidence":0.92}'
      ),
      curl: makeCurl('claude-3-sonnet', 'Summarize the success payload in one sentence with confidence.'),
    },
    extractUser: {
      prompt: makeChatPrompt(
        'GPT-4-turbo',
        'You extract structured user info from resumes.',
        'Resume text: Jane Doe, Senior PM at Example Inc...'
      ),
      curl: makeCurl('gpt-4-turbo', 'Return JSON fields full_name, title, skills extracted from resume.'),
    },
    standaloneLlmFunction: {
      prompt: makeChatPrompt(
        'GPT-4o-mini',
        'You answer short standalone user prompts with brief helpful replies.',
        'User: Give me a friendly greeting.'
      ),
      curl: makeCurl('gpt-4o-mini', 'Respond succinctly to the provided user prompt.'),
    },
  };
}

/**
 * Create default mock runtime configuration
 */
export function createMockRuntimeConfig(
  options?: {
    cacheHitRate?: number;
    errorRate?: number;
    verboseLogging?: boolean;
    speedMultiplier?: number;
  }
): MockRuntimeConfig {
  const testCases = createMockTestCases();

  const config: MockRuntimeConfig = {
    workflows: createMockWorkflows(),
    functions: [], // No standalone functions in this config
    testCases,
    nodeOutputs: createOutputGenerators(),
    executionBehavior: {
      cacheHitRate: options?.cacheHitRate ?? 0.3,
      errorRate: options?.errorRate ?? 0.1,
      verboseLogging: options?.verboseLogging ?? true,
      speedMultiplier: options?.speedMultiplier ?? 1,
      nodeDelays: {
        llm_function: () => 1500 + Math.random() * 1000,
        function: () => 400 + Math.random() * 300,
        conditional: () => 300 + Math.random() * 200,
      },
    },
    bamlFiles: createBAMLFiles(testCases),
    functionPrompts: createFunctionPrompts(),
  };

  validateMockRuntimeConfig(config);

  return config;
}

// Ensure every declared test references an existing mock test case payload.
function validateMockRuntimeConfig(config: MockRuntimeConfig): void {
  const missingTestCaseBindings: string[] = [];

  for (const file of config.bamlFiles) {
    for (const test of file.tests ?? []) {
      const linkedTestCases = config.testCases[test.functionName] || [];
      const hasMatchingTest = linkedTestCases.some((tc) => tc.name === test.name);
      if (!hasMatchingTest) {
        missingTestCaseBindings.push(
          `${test.functionName} (${file.path} -> ${test.name})`
        );
      }
    }
  }

  if (missingTestCaseBindings.length) {
    throw new Error(
      `[mock-config] Missing mock test cases for: ${missingTestCaseBindings.join(', ')}`
    );
  }
}
