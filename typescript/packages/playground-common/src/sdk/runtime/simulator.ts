/**
 * Execution Simulator for Mock Runtime
 *
 * Simulates workflow execution with realistic behavior including:
 * - Branching (conditional nodes)
 * - Delays based on node type
 * - Cache hits
 * - Errors
 * - Logging
 */

import type { GraphNode } from '../types';
import type { RichExecutionEvent, FunctionWithCallGraph } from '../interface';
import type { MockRuntimeConfig } from '../mock-config/types';
import type { LogEntry } from '../types';

/**
 * Simulate workflow execution following the graph structure
 */
export async function* simulateExecution(
  workflow: FunctionWithCallGraph,
  config: MockRuntimeConfig,
  inputs: Record<string, unknown>,
  executionId: string,
  startFromNodeId?: string
): AsyncGenerator<RichExecutionEvent> {
  const visited = new Set<string>();
  let currentNodes = [startFromNodeId || workflow.entryPoint];
  let iterationCount = 0;
  const maxIterations = 20;

  // Context accumulates throughout execution
  const context: Record<string, unknown> = { ...inputs };

  if (startFromNodeId && startFromNodeId !== workflow.entryPoint) {
    console.log(`⏩ Starting execution from node: ${startFromNodeId}`);
  }

  while (currentNodes.length > 0 && iterationCount < maxIterations) {
    iterationCount++;
    const nextNodes: string[] = [];

    for (const nodeId of currentNodes) {
      const node = workflow.nodes.find((n) => n.id === nodeId);
      if (!node) continue;

      // Skip if already visited (unless it's a loop)
      if (visited.has(nodeId) && node.type !== 'loop') {
        continue;
      }

      visited.add(nodeId);

      // Execute node
      const result = yield* executeNode(
        node,
        executionId,
        context,
        workflow,
        config
      );

      if (result.error) {
        // Stop execution on error
        return;
      }

      // Merge outputs into context
      if (result.outputs) {
        Object.assign(context, result.outputs);
      }

      // Determine next nodes based on node type
      const outgoingEdges = workflow.edges.filter((e) => e.source === nodeId);

      if (node.type === 'conditional' && result.outputs?.condition) {
        // Follow the branch that matches the condition
        const chosenEdge = outgoingEdges.find(
          (e) => e.label === result.outputs?.condition
        );
        if (chosenEdge) {
          nextNodes.push(chosenEdge.target);

          if (config.executionBehavior.verboseLogging) {
            yield {
              type: 'log',
              nodeId,
              timestamp: Date.now(),
              iteration: 0,
              executionId,
              level: 'info',
              message: `Branch: ${result.outputs.condition} → ${chosenEdge.target}`,
            };
          }
        }
      } else if (node.type === 'return') {
        // End execution
        break;
      } else {
        // Follow all outgoing edges
        nextNodes.push(...outgoingEdges.map((e) => e.target));
      }
    }

    currentNodes = nextNodes;

    // Small delay between execution steps
    await delay(100 * config.executionBehavior.speedMultiplier);
  }
}

/**
 * Execute a single node with realistic simulation
 */
async function* executeNode(
  node: GraphNode,
  executionId: string,
  context: Record<string, unknown>,
  workflow: FunctionWithCallGraph,
  config: MockRuntimeConfig
): AsyncGenerator<
  RichExecutionEvent,
  { outputs?: Record<string, unknown>; error?: Error },
  undefined
> {
  // Capture inputs at the start
  const nodeInputs = { ...context };

  // Emit start event
  yield {
    type: 'node.enter',
    nodeId: node.id,
    timestamp: Date.now(),
    iteration: 0,
    executionId,
    inputs: nodeInputs,
  };

  // Check for cache hit
  const shouldUseCache =
    Math.random() < config.executionBehavior.cacheHitRate;
  if (shouldUseCache) {
    const cachedOutputs = generateOutputs(node, workflow, context, config);

    // Log cache hit if verbose logging enabled
    if (config.executionBehavior.verboseLogging) {
      yield {
        type: 'log',
        nodeId: node.id,
        timestamp: Date.now(),
        iteration: 0,
        executionId,
        level: 'info',
        message: 'Cache hit - using cached result',
      };
    }

    yield {
      type: 'node.exit',
      nodeId: node.id,
      timestamp: Date.now(),
      iteration: 0,
      executionId,
      outputs: cachedOutputs,
      duration: 50, // Cached is fast
    };

    return { outputs: cachedOutputs };
  }

  // Simulate processing with logs
  const duration = getNodeDuration(node.type, config);
  const startTime = Date.now();

  // Generate realistic logs during execution
  const logCount = node.type === 'llm_function' ? 3 : 1;
  for (let i = 0; i < logCount; i++) {
    await delay((duration / logCount) * config.executionBehavior.speedMultiplier);

    if (config.executionBehavior.verboseLogging) {
      yield {
        type: 'log',
        nodeId: node.id,
        timestamp: Date.now(),
        iteration: 0,
        executionId,
        level: 'info',
        message: getLogMessage(node, i, logCount),
      };
    }
  }

  // Simulate errors (based on configured error rate)
  const shouldError = Math.random() < config.executionBehavior.errorRate;

  // Generate outputs
  const outputs = generateOutputs(node, workflow, context, config);
  const actualDuration = Date.now() - startTime;

  if (shouldError) {
    const error = new Error(getErrorMessage(node));
    yield {
      type: 'node.exit',
      nodeId: node.id,
      timestamp: Date.now(),
      iteration: 0,
      executionId,
      duration: actualDuration,
      error: {
        message: error.message,
        code: 'SIMULATION_ERROR',
        stack: error.stack,
      },
    };
    return { error };
  }

  yield {
    type: 'node.exit',
    nodeId: node.id,
    timestamp: Date.now(),
    iteration: 0,
    executionId,
    outputs,
    duration: actualDuration,
  };

  return { outputs };
}

/**
 * Generate mock outputs using the configured output generators
 */
function generateOutputs(
  node: GraphNode,
  workflow: FunctionWithCallGraph,
  context: Record<string, unknown>,
  config: MockRuntimeConfig
): Record<string, unknown> {
  // Try workflow-specific generator first
  const workflowSpecificKey = `${workflow.id}.${node.id}`;
  const workflowGenerator = config.nodeOutputs[workflowSpecificKey];
  if (workflowGenerator) {
    return workflowGenerator(context, { ...context });
  }

  // Try node-specific generator
  const nodeGenerator = config.nodeOutputs[node.id];
  if (nodeGenerator) {
    return nodeGenerator(context, { ...context });
  }

  // Fallback to generic outputs
  return { completed: true, timestamp: Date.now() };
}

function getNodeDuration(
  nodeType: GraphNode['type'],
  config: MockRuntimeConfig
): number {
  const delayFn = config.executionBehavior.nodeDelays[nodeType];
  if (delayFn) {
    return delayFn();
  }

  // Default delays
  switch (nodeType) {
    case 'llm_function':
      return 1500 + Math.random() * 1000;
    case 'conditional':
      return 300 + Math.random() * 200;
    case 'function':
      return 400 + Math.random() * 300;
    default:
      return 500 + Math.random() * 500;
  }
}

function getLogMessage(
  node: GraphNode,
  step: number,
  totalSteps: number
): string {
  if (node.type === 'llm_function') {
    const messages = [
      `Preparing prompt for ${node.label}`,
      `Calling LLM API (model: gpt-4)`,
      `Received and processing response`,
    ];
    return messages[step] || `Processing ${node.label}...`;
  }

  if (node.type === 'conditional') {
    return `Evaluating condition: ${node.label}`;
  }

  return `Executing ${node.label}`;
}

function getErrorMessage(node: GraphNode): string {
  const errors = [
    `Timeout while executing ${node.label}`,
    `Invalid response from ${node.label}`,
    `Resource not found in ${node.label}`,
    `Rate limit exceeded in ${node.label}`,
  ];
  return errors[Math.floor(Math.random() * errors.length)] ?? '';
}

function createLog(
  executionId: string,
  level: 'debug' | 'info' | 'warn' | 'error',
  message: string
): LogEntry {
  return {
    timestamp: Date.now(),
    level,
    message,
    executionId,
  };
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
