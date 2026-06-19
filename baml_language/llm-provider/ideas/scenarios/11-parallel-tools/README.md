# 11 — Parallel tool calls

When a model emits several tool calls in one assistant turn, the app wants to run them concurrently and feed all results back together — in the *original* order, so that Anthropic's positional `tool_result` matching and prompt caching stay stable. This scenario shows that under the proposed model, parallelism is purely a `ctx.dispatch` concern: it sits ABOVE the `ToolCall`/`ToolResult` seam, so the provider tool loops (`begin`/`step`/`submit`) are reused byte-for-byte from scenario 09. The four files demonstrate run-all vs stop-on-first-error, an effect-aware strategy that fans out read-only tools and sequences writes (via a net-new `Tool.effect` annotation, since no wire metadata marks a tool read-only), and a semaphore-capped variant for tool-level rate limits. The cost is a host concurrency surface BAML doesn't have today.

Background: background/02-tools-and-agents.md → ## 3. ◆ Parallel tool calls
