/**
 * Unified Type System - Execution Event Types
 *
 * Rich execution events for run_tests_v2 WASM API
 * All events must have node IDs, timestamps, and iterations for graph mapping
 */

import type { LLMFailureInfo, LLMResponseInfo, PromptInfo, SpanInfo, WatchNotification, TestResponseData } from './types';

// ============================================================================
// RICH EXECUTION EVENTS (for run_tests_v2)
// ============================================================================

/**
 * Base event properties
 * All events must have these fields for proper graph mapping
 */
export interface BaseExecutionEvent {
  /** Unique identifier for the node (function or block) */
  nodeId: string;
  /** Timestamp in milliseconds (for ordering) */
  timestamp: number;
  /** Iteration count (for loops/cycles) */
  iteration: number;
  /** Execution ID to group related events */
  executionId: string;
}

/**
 * Node entry event (function or block entered)
 */
export interface NodeEnterEvent extends BaseExecutionEvent {
  type: 'node.enter';
  /** Input values for this invocation */
  inputs?: Record<string, unknown>;
  /** Parent node ID (if this is a nested call) */
  parentNodeId?: string;
}

/**
 * Node exit event (function or block exited)
 */
export interface NodeExitEvent extends BaseExecutionEvent {
  type: 'node.exit';
  /** Output values from this invocation */
  outputs?: Record<string, unknown>;
  /** Duration in milliseconds */
  duration: number;
  /** Error if failed */
  error?: {
    message: string;
    code?: string;
    stack?: string;
  };
  /** Full response data (converted from WASM) for test executions */
  responseData?: TestResponseData;
}

/**
 * Block-specific events (if, loop, return, etc.)
 */
export type BlockType = 'if' | 'loop' | 'return' | 'assignment' | 'expression';

export interface BlockEnterEvent extends BaseExecutionEvent {
  type: 'block.enter';
  blockType: BlockType;
  /** User annotation (from comment like # check blah) */
  annotation?: string;
  /** Condition value (for if blocks) */
  conditionValue?: boolean;
}

export interface BlockExitEvent extends BaseExecutionEvent {
  type: 'block.exit';
  blockType: BlockType;
  /** Number of iterations (for loop blocks) */
  iterationCount?: number;
}

/**
 * LLM request event (before LLM call)
 */
export interface LLMRequestEvent extends BaseExecutionEvent {
  type: 'llm.request';
  /** Actual client used (may differ from definition) */
  actualClient: string;
  /** Actual model used */
  actualModel?: string;
  /** Prompt being sent */
  prompt: PromptInfo;
  /** Request configuration */
  config?: {
    temperature?: number;
    maxTokens?: number;
    [key: string]: unknown;
  };
}

/**
 * LLM response event (after LLM responds)
 */
export interface LLMResponseEvent extends BaseExecutionEvent {
  type: 'llm.response';
  /** LLM response info */
  response: LLMResponseInfo;
  /** Actual output type (may differ from definition) */
  actualOutputType?: string;
  /** Parsed output value */
  parsedOutput?: unknown;
}

/**
 * LLM failure event (LLM call failed)
 */
export interface LLMFailureEvent extends BaseExecutionEvent {
  type: 'llm.failure';
  /** LLM failure info */
  failure: LLMFailureInfo;
}

/**
 * Partial response event (streaming)
 */
export interface PartialResponseEvent extends BaseExecutionEvent {
  type: 'partial.response';
  /** Partial content */
  partialContent: string;
  /** Whether this is the final chunk */
  isFinal: boolean;
}

/**
 * Watch notification event (from BAML watch blocks)
 */
export interface WatchNotificationEvent extends BaseExecutionEvent {
  type: 'watch.notification';
  notification: WatchNotification;
}

/**
 * Code highlight event (for IDE integration)
 */
export interface HighlightEvent extends BaseExecutionEvent {
  type: 'highlight';
  spans: SpanInfo[];
}

/**
 * Log event (debug/info/warn/error)
 */
export interface LogEvent extends BaseExecutionEvent {
  type: 'log';
  level: 'debug' | 'info' | 'warn' | 'error';
  message: string;
  metadata?: Record<string, unknown>;
}

/**
 * Union of all execution events
 */
export type RichExecutionEvent =
  | NodeEnterEvent
  | NodeExitEvent
  | BlockEnterEvent
  | BlockExitEvent
  | LLMRequestEvent
  | LLMResponseEvent
  | LLMFailureEvent
  | PartialResponseEvent
  | WatchNotificationEvent
  | HighlightEvent
  | LogEvent;
