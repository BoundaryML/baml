import type React from 'react';
import type { TestState } from '../../../atoms';

interface TestResultStatsProps {
  response?: TestState;
}

export const TestResultStats: React.FC<TestResultStatsProps> = ({ response }) => {
  const numberFormatter = new Intl.NumberFormat();

  // Extract stats from the response
  const getResponseStats = () => {
    if (!response || response.status !== 'done') {
      return null;
    }

    const wasmResponse = response.response;

    // Try to get stats from successful LLM response first
    const llmResponse = wasmResponse.llm_response();
    if (llmResponse) {
      return {
        latency: response.latency_ms,
        clientName: llmResponse.client_name(),
        model: llmResponse.model,
        inputTokens: llmResponse.input_tokens ? Number(llmResponse.input_tokens) : null,
        outputTokens: llmResponse.output_tokens ? Number(llmResponse.output_tokens) : null,
        totalTokens: llmResponse.total_tokens ? Number(llmResponse.total_tokens) : null,
        stopReason: llmResponse.stop_reason || null,
        startTime: Number(llmResponse.start_time_unix_ms),
      };
    }

    // Fall back to LLM failure data if available
    const llmFailure = wasmResponse.llm_failure();
    if (llmFailure) {
      return {
        latency: response.latency_ms,
        clientName: llmFailure.client_name(),
        model: llmFailure.model || null,
        inputTokens: null,
        outputTokens: null,
        totalTokens: null,
        stopReason: null,
        startTime: Number(llmFailure.start_time_unix_ms),
      };
    }

    // Return basic info if no LLM data available
    return {
      latency: response.latency_ms,
      clientName: null,
      model: null,
      inputTokens: null,
      outputTokens: null,
      totalTokens: null,
      stopReason: null,
      startTime: null,
    };
  };

  const stats = getResponseStats();

  // Don't render if no stats available
  if (!stats) {
    return null;
  }

  return (
    <div className="flex flex-row gap-4 justify-start items-stretch px-2 py-2 text-xs border border-border bg-muted text-muted-foreground rounded-b w-full">
      <div className="flex flex-wrap gap-y-2 gap-x-4 w-full">
        {/* Client */}
        {stats.clientName && (
          <div className="flex flex-col items-start min-w-[80px]">
            <span className="text-muted-foreground/60">Client</span>
            <span className="font-medium font-mono text-xs">
              {stats.clientName}
            </span>
          </div>
        )}

        {/* Model */}
        {stats.model && (
          <div className="flex flex-col items-start min-w-[100px]">
            <span className="text-muted-foreground/60">Model</span>
            <span className="font-medium font-mono text-xs">
              {stats.model}
            </span>
          </div>
        )}

        {/* Timing */}
        <div className="flex flex-col items-start min-w-[60px]">
          <span className="text-muted-foreground/60">Timing</span>
          <span className="font-medium">
            {numberFormatter.format(stats.latency)}ms
          </span>
        </div>

        {/* Input Tokens */}
        {stats.inputTokens !== null && (
          <div className="flex flex-col items-start min-w-[50px]">
            <span className="text-muted-foreground/60">Input</span>
            <span className="font-medium">
              {numberFormatter.format(stats.inputTokens)}t
            </span>
          </div>
        )}

        {/* Output Tokens */}
        {stats.outputTokens !== null && (
          <div className="flex flex-col items-start min-w-[50px]">
            <span className="text-muted-foreground/60">Output</span>
            <span className="font-medium">
              {numberFormatter.format(stats.outputTokens)}t
            </span>
          </div>
        )}

        {/* Total Tokens (if available and different from sum) */}
        {stats.totalTokens !== null &&
         stats.totalTokens !== (stats.inputTokens || 0) + (stats.outputTokens || 0) && (
          <div className="flex flex-col items-start min-w-[50px]">
            <span className="text-muted-foreground/60">Total</span>
            <span className="font-medium">
              {numberFormatter.format(stats.totalTokens)}t
            </span>
          </div>
        )}

        {/* Stop Reason */}
        {stats.stopReason && (
          <div className="flex flex-col items-start min-w-[70px]">
            <span className="text-muted-foreground/60">Stop</span>
            <span className="font-medium text-xs">
              {stats.stopReason}
            </span>
          </div>
        )}
      </div>
    </div>
  );
};