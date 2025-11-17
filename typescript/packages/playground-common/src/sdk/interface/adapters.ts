/**
 * Unified Type System - WASM Adapters & Mock Generators
 *
 * This file contains:
 * 1. WasmTypeAdapter - Converts WASM types to unified interface types
 * 2. Mock generators - Create mock data for MockBamlRuntime
 */

import type {
  WasmFunction,
  WasmTestCase,
  WasmSpan,
  WasmParam,
  WasmParentFunction,
  WasmPrompt,
  WasmChatMessage,
  WasmChatMessagePart,
  WasmTestResponse,
  WasmFunctionResponse,
  WasmParsedTestResponse,
  WasmLLMResponse,
  WasmLLMFailure,
  WasmScope,
  WasmRuntime,
  TestStatus as WasmTestStatus,
} from '@gloo-ai/baml-schema-wasm-web/baml_schema_build';
import { WasmFunctionKind } from '@gloo-ai/baml-schema-wasm-web/baml_schema_build';

import type {
  SpanInfo,
  ParentFunctionInfo,
  ParameterInfo,
  TestCaseMetadata,
  FunctionMetadata,
  OrchestrationScope,
  PromptInfo,
  ChatMessage,
  ChatMessagePart,
  ParsedTestResponse,
  LLMResponseInfo,
  LLMFailureInfo,
  TestStatus,
  TestExecutionResult,
  TestResponseData,
} from './types';

// ============================================================================
// WASM → UNIFIED TYPE ADAPTERS
// ============================================================================

/**
 * Adapter class for converting WASM types to unified interface types
 */
export class WasmTypeAdapter {
  constructor(
    private wasmModule: typeof import('@gloo-ai/baml-schema-wasm-web/baml_schema_build')
  ) {}

  // ============================================================================
  // SPAN & LOCATION ADAPTERS
  // ============================================================================

  convertSpan(wasmSpan: WasmSpan): SpanInfo {
    return {
      filePath: wasmSpan.file_path,
      start: wasmSpan.start,
      end: wasmSpan.end,
      startLine: wasmSpan.start_line,
      startColumn: wasmSpan.start_column,
      endLine: wasmSpan.end_line,
      endColumn: wasmSpan.end_column,
    };
  }

  convertParentFunction(wasmParent: WasmParentFunction): ParentFunctionInfo {
    return {
      name: wasmParent.name,
      start: wasmParent.start,
      end: wasmParent.end,
    };
  }

  // ============================================================================
  // PARAMETER ADAPTERS
  // ============================================================================

  convertParam(wasmParam: WasmParam): ParameterInfo {
    return {
      name: wasmParam.name,
      value: wasmParam.value,
      error: wasmParam.error,
    };
  }

  // ============================================================================
  // TEST CASE ADAPTERS
  // ============================================================================

  convertTestCase(wasmTestCase: WasmTestCase): TestCaseMetadata {
    const span = this.convertSpan(wasmTestCase.span);
    const parentFunctions = wasmTestCase.parent_functions.map(pf => this.convertParentFunction(pf));

    return {
      name: wasmTestCase.name,
      inputs: wasmTestCase.inputs.map(p => this.convertParam(p)),
      error: wasmTestCase.error,
      span,
      parentFunctions,

      // Backward compatibility fields
      id: `${wasmTestCase.name}_${Date.now()}`,
      source: 'test' as const,
      functionId: parentFunctions[0]?.name || '',
      filePath: span.filePath,
      status: wasmTestCase.error ? ('failing' as const) : ('unknown' as const),
    };
  }

  // ============================================================================
  // ORCHESTRATION ADAPTERS
  // ============================================================================

  convertOrchestrationScope(wasmScope: WasmScope): OrchestrationScope {
    return {
      name: wasmScope.name(),
      scopeInfo: wasmScope.get_orchestration_scope_info(),
    };
  }

  // ============================================================================
  // FUNCTION ADAPTERS
  // ============================================================================

  convertFunction(wasmFn: WasmFunction, runtime: WasmRuntime): FunctionMetadata {
    const flavor: FunctionMetadata['functionFlavor'] =
      wasmFn.function_type === WasmFunctionKind.Llm ? 'llm' : 'expr';

    let type: FunctionMetadata['type'] =
      flavor === 'llm' ? 'llm_function' : 'function';
    let clientName: string | undefined;

    if (flavor === 'llm') {
      try {
        const rawClient = wasmFn.client_name(runtime);
        clientName = rawClient || undefined;
        if (!clientName) {
          type = 'workflow';
        }
      } catch {
        type = 'workflow';
      }
    }

    // TODO: Re-enable orchestration graph when needed
    // Temporarily disabled - orchestration graph needs migration
    const orchestrationGraph = undefined;

    return {
      name: wasmFn.name,
      type,
      functionFlavor: flavor,
      span: this.convertSpan(wasmFn.span),
      signature: wasmFn.signature,
      testSnippet: wasmFn.test_snippet,
      testCases: wasmFn.test_cases.map(tc => this.convertTestCase(tc)),
      clientName,
      orchestrationGraph,
    };
  }

  convertExprFunction(wasmFn: WasmFunction): FunctionMetadata {
    return {
      name: wasmFn.name,
      type: 'function',
      functionFlavor: 'expr',
      span: this.convertSpan(wasmFn.span),
      signature: wasmFn.signature,
      testSnippet: wasmFn.test_snippet,
      testCases: wasmFn.test_cases.map(tc => this.convertTestCase(tc)),
      clientName: undefined,
      orchestrationGraph: undefined,
    };
  }

  // ============================================================================
  // PROMPT ADAPTERS
  // ============================================================================

  convertChatMessagePart(wasmPart: WasmChatMessagePart, prompt: WasmPrompt): ChatMessagePart {
    if (wasmPart.is_text()) {
      return {
        type: 'text',
        content: wasmPart.as_text() || '',
      };
    }

    const media = wasmPart.as_media();
    if (media) {
      // Determine media type based on is_* methods
      let mediaType: ChatMessagePart['type'] = 'text';
      if (wasmPart.is_image()) mediaType = 'image';
      else if (wasmPart.is_audio()) mediaType = 'audio';
      else if (wasmPart.is_pdf()) mediaType = 'pdf';
      else if (wasmPart.is_video()) mediaType = 'video';

      const jsonMeta = wasmPart.json_meta(prompt);
      return {
        type: mediaType,
        content: media.content,
        metadata: jsonMeta ? JSON.parse(jsonMeta) : undefined,
      };
    }

    // Fallback
    return {
      type: 'text',
      content: '',
    };
  }

  convertChatMessage(wasmMsg: WasmChatMessage, prompt: WasmPrompt): ChatMessage {
    return {
      role: wasmMsg.role,
      parts: wasmMsg.parts.map(part => this.convertChatMessagePart(part, prompt)),
    };
  }

  convertPrompt(wasmPrompt: WasmPrompt): PromptInfo {
    if (wasmPrompt.is_chat()) {
      const messages = wasmPrompt.as_chat();
      return {
        type: 'chat',
        clientName: wasmPrompt.client_name,
        messages: messages ? messages.map(msg => this.convertChatMessage(msg, wasmPrompt)) : [],
      };
    } else {
      // Completion prompt - WASM doesn't expose text directly, but we can infer
      return {
        type: 'completion',
        clientName: wasmPrompt.client_name,
        text: '', // TODO: Extract from WASM if available
      };
    }
  }

  // ============================================================================
  // TEST EXECUTION RESULT ADAPTERS
  // ============================================================================

  convertParsedTestResponse(wasmParsed: WasmParsedTestResponse): ParsedTestResponse {
    return {
      value: wasmParsed.value,
      checkCount: wasmParsed.check_count,
      explanation: wasmParsed.explanation,
    };
  }

  convertLLMResponse(wasmResponse: WasmLLMResponse): LLMResponseInfo {
    return {
      clientName: wasmResponse.client_name(),
      model: wasmResponse.model,
      content: wasmResponse.content,
      prompt: this.convertPrompt(wasmResponse.prompt()),
      inputTokens: wasmResponse.input_tokens ? Number(wasmResponse.input_tokens) : undefined,
      outputTokens: wasmResponse.output_tokens ? Number(wasmResponse.output_tokens) : undefined,
      totalTokens: wasmResponse.total_tokens ? Number(wasmResponse.total_tokens) : undefined,
      stopReason: wasmResponse.stop_reason,
      startTimeUnixMs: Number(wasmResponse.start_time_unix_ms),
      latencyMs: Number(wasmResponse.latency_ms),
    };
  }

  convertLLMFailure(wasmFailure: WasmLLMFailure): LLMFailureInfo {
    return {
      clientName: wasmFailure.client_name(),
      model: wasmFailure.model,
      message: wasmFailure.message,
      code: wasmFailure.code,
      prompt: this.convertPrompt(wasmFailure.prompt()),
      startTimeUnixMs: Number(wasmFailure.start_time_unix_ms),
      latencyMs: Number(wasmFailure.latency_ms),
    };
  }

  convertTestStatus(wasmStatus: WasmTestStatus): TestStatus {
    const statusMap: Record<WasmTestStatus, TestStatus> = {
      [this.wasmModule.TestStatus.Passed]: 'passed',
      [this.wasmModule.TestStatus.LLMFailure]: 'llm_failed',
      [this.wasmModule.TestStatus.ParseFailure]: 'parse_failed',
      [this.wasmModule.TestStatus.FinishReasonFailed]: 'finish_reason_failed',
      [this.wasmModule.TestStatus.ConstraintsFailed]: 'constraints_failed',
      [this.wasmModule.TestStatus.AssertFailed]: 'assert_failed',
      [this.wasmModule.TestStatus.UnableToRun]: 'unable_to_run',
    };

    return statusMap[wasmStatus] || 'unable_to_run';
  }

  convertTestResponse(wasmResponse: WasmTestResponse): TestExecutionResult {
    const pair = wasmResponse.func_test_pair();
    const status = this.convertTestStatus(wasmResponse.status());
    const parsedResponse = wasmResponse.parsed_response();
    const llmResponse = wasmResponse.llm_response();
    const llmFailure = wasmResponse.llm_failure();

    return {
      functionName: pair.function_name,
      testName: pair.test_name,
      status,
      parsedResponse: parsedResponse ? this.convertParsedTestResponse(parsedResponse) : undefined,
      llmResponse: llmResponse ? this.convertLLMResponse(llmResponse) : undefined,
      llmFailure: llmFailure ? this.convertLLMFailure(llmFailure) : undefined,
      failureMessage: wasmResponse.failure_message() || undefined,
      traceUrl: wasmResponse.trace_url() || undefined,
    };
  }

  /**
   * Convert WASM test/function response to plain object data
   * This is used to store responses without WASM dependencies
   */
  convertResponseToData(wasmResponse: WasmTestResponse | WasmFunctionResponse): TestResponseData {
    const parsedResponse = wasmResponse.parsed_response();
    const llmResponse = wasmResponse.llm_response();
    const llmFailure = wasmResponse.llm_failure();
    const failureMessage = 'failure_message' in wasmResponse ? wasmResponse.failure_message() : undefined;

    // Handle both WasmTestResponse (returns WasmParsedTestResponse) and WasmFunctionResponse (returns string)
    let convertedParsedResponse: ParsedTestResponse | undefined;
    if (parsedResponse) {
      if (typeof parsedResponse === 'string') {
        // WasmFunctionResponse case - string response
        convertedParsedResponse = { value: parsedResponse, checkCount: 0 };
      } else {
        // WasmTestResponse case - structured response
        convertedParsedResponse = this.convertParsedTestResponse(parsedResponse);
      }
    }

    return {
      parsed_response: convertedParsedResponse,
      llm_response: llmResponse ? this.convertLLMResponse(llmResponse) : undefined,
      llm_failure: llmFailure ? this.convertLLMFailure(llmFailure) : undefined,
      failure_message: failureMessage || undefined,
    };
  }
}

// ============================================================================
// MOCK TYPE GENERATORS (No WASM dependency!)
// ============================================================================

/**
 * Create mock span
 */
export function createMockSpan(filePath: string): SpanInfo {
  return {
    filePath,
    start: 0,
    end: 100,
    startLine: 1,
    startColumn: 0,
    endLine: 10,
    endColumn: 0,
  };
}

/**
 * Create mock function metadata
 * Generates pure TypeScript types - no WASM objects
 */
export function createMockFunction(
  name: string,
  type: FunctionMetadata['type'],
  filePath: string,
  options?: {
    clientName?: string;
    testCases?: TestCaseMetadata[];
    flavor?: FunctionMetadata['functionFlavor'];
  }
): FunctionMetadata {
  return {
    name,
    type,
    functionFlavor: options?.flavor ?? (type === 'llm_function' ? 'llm' : 'expr'),
    span: createMockSpan(filePath),
    signature: `function ${name}(...) -> ...`,
    testSnippet: `test ${name}_test {\n  functions [${name}]\n  args { }\n}`,
    testCases: options?.testCases || [],
    clientName: options?.clientName,
    orchestrationGraph: [],
  };
}

/**
 * Create mock test case metadata
 */
export function createMockTestCase(
  name: string,
  parentFunctionName: string,
  filePath: string,
  inputs?: ParameterInfo[]
): TestCaseMetadata {
  return {
    id: `${parentFunctionName}_${name}`,
    name,
    source: 'test',
    functionId: parentFunctionName,
    filePath,
    inputs: inputs || [],
    span: createMockSpan(filePath),
    parentFunctions: [
      {
        name: parentFunctionName,
        start: 0,
        end: 100,
      },
    ],
  };
}

/**
 * Create mock test execution result
 */
export function createMockTestResult(
  functionName: string,
  testName: string,
  status: TestStatus = 'passed'
): TestExecutionResult {
  return {
    functionName,
    testName,
    status,
    parsedResponse: status === 'passed' ? {
      value: '{"mocked": true}',
      checkCount: 0,
    } : undefined,
    llmResponse: status === 'passed' ? {
      clientName: 'mock-client',
      model: 'mock-model',
      content: 'Mock response',
      prompt: {
        type: 'chat',
        clientName: 'mock-client',
        messages: [],
      },
      startTimeUnixMs: Date.now(),
      latencyMs: 500,
    } : undefined,
    llmFailure: status === 'llm_failed' ? {
      clientName: 'mock-client',
      message: 'Mock failure',
      code: 'MOCK_ERROR',
      prompt: {
        type: 'chat',
        clientName: 'mock-client',
        messages: [],
      },
      startTimeUnixMs: Date.now(),
      latencyMs: 500,
    } : undefined,
  };
}

/**
 * Create mock prompt
 */
export function createMockPrompt(
  type: PromptInfo['type'],
  clientName: string
): PromptInfo {
  if (type === 'chat') {
    return {
      type: 'chat',
      clientName,
      messages: [
        {
          role: 'user',
          parts: [
            {
              type: 'text',
              content: 'Mock prompt',
            },
          ],
        },
      ],
    };
  } else {
    return {
      type: 'completion',
      clientName,
      text: 'Mock prompt text',
    };
  }
}
