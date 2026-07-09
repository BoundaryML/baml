// scenario 09 — tool calling: ../../../../../llm-provider/ideas/scenarios/09-tool-calling/implement.baml
//   implementation.baml — LIBRARY-author code.
//
// The design's implement.baml hand-rolled three providers (OpenAI/Anthropic/
// Gemini) each `implements HttpProvider + Tools`, plus a `ToolFallback`
// combinator and net-new host surface (`ExecutionContext.dispatch`/`deps`,
// `reflect.type_to_json_schema`/`type_to_gemini_openapi`). On the REAL surface
// all of that already ships in `baml.ai`:
//   - `Tool { name, description, parameters, params_type?, handoff? }` + `Tool.from_type`
//     (one BAML `type` -> the wire schema; the three dialects live inside each
//     native provider's `begin`, below the seam — REALIZED.md #09),
//   - the `Tools` interface (`begin`/`step`/`submit` + the default `run_tools`),
//   - `OpenAi`/`Anthropic`/`Gemini` each `implements Tools` natively,
//   - `ToolLoop` (the agentic loop AS a provider — the design's `.run_tools`
//     companion, now a plain `HttpProvider` call on a ToolLoop client).
// So the only genuinely library-author code THIS scenario still owns is the one
// concrete typed error it raises on purpose: the app-policy denial of a tool
// call (the design's `ToolCallDenied`, an approval-gate `ToolError`).

// App-policy denial of a tool call (the design's ApprovedSupport approval gate).
// Surfaces mid-loop, so it is a `ToolError` the app catches concretely — most-
// specific first, before any interface arm. `implements Failure` answers the
// uniform decision axis (a policy refusal, not retryable/network/rate-limit).
class s09_ToolCallDenied {
    tool_name: string,

    implements baml.errors.Failure {
        function is_retryable(self) -> bool { false }
        function is_effectful(self) -> bool { false }
        function is_policy_refusal(self) -> bool { true }
        function is_resumable(self) -> bool { false }
        function is_unsupported(self) -> bool { false }
    }
    implements baml.errors.ToolError {
        function is_network_error(self) -> bool { false }
    }
}
