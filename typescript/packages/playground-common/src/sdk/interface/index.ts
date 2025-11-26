/**
 * Unified Type System - Interface Layer
 *
 * This is the interface layer between WASM types and TypeScript types.
 * All atoms and UI components should import from this layer, never directly from WASM.
 *
 * Key principles:
 * - Pure TypeScript types (no WASM dependencies)
 * - Works for both BamlRuntime and MockBamlRuntime
 * - Safe for atoms and UI components
 * - Adapters convert WASM types to these interfaces
 */

// ============================================================================
// CORE TYPES
// ============================================================================

export type {
  // Span & Location
  SpanInfo,
  ParentFunctionInfo,

  // Parameters
  ParameterInfo,

  // Functions
  FunctionMetadata,
  TestCaseMetadata,
  OrchestrationScope,
  FunctionWithCallGraph,

  // Call Graph
  CallGraphNode,
  BlockType,

  // Workflow/Graph
  NodeType,
  GraphNode,
  GraphEdge,
  Parameter,
  WorkflowDefinition,

  // Prompts
  PromptType,
  PromptInfo,
  ChatMessage,
  ChatMessagePart,

  // Test Execution
  TestStatus,
  ParsedTestResponse,
  LLMResponseInfo,
  LLMFailureInfo,
  TestExecutionResult,
  TestExecutionContext,
  WatchNotification,
  TestResponseData,

  // Watch Notification Value Types
  WatchEventSpan,
  WatchHeaderValue,
  WatchHeaderStoppedValue,
  WatchStreamStartValue,
  WatchStreamUpdateValue,
  WatchStreamEndValue,
  WatchVariableValue,
  WatchNotificationValue,
  RichWatchNotification,

  // Function Call
  FunctionCall,
} from './types';

// ============================================================================
// EXECUTION EVENTS
// ============================================================================

export type {
  // Base Event
  BaseExecutionEvent,

  // Node Events
  NodeEnterEvent,
  NodeExitEvent,

  // Block Events
  BlockEnterEvent,
  BlockExitEvent,

  // LLM Events
  LLMRequestEvent,
  LLMResponseEvent,
  LLMFailureEvent,

  // Other Events
  PartialResponseEvent,
  HeaderEnterEvent,
  HeaderExitEvent,
  VariableUpdateEvent,
  HighlightEvent,
  LogEvent,

  // Union Type
  RichExecutionEvent,
} from './events';

// ============================================================================
// ADAPTERS & GENERATORS
// ============================================================================

export {
  // WASM Adapter
  WasmTypeAdapter,

  // Mock Generators
  createMockSpan,
  createMockFunction,
  createMockTestCase,
  createMockTestResult,
  createMockPrompt,
} from './adapters';
