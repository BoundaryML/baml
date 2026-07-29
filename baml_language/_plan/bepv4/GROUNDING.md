# Grounding rule + audit

**Rule:** every code snippet in a BEP page must be grounded in
`crates/baml_tests/baml_src_temp2` — same names, same shapes, same behavior —
ideally in `ns_ai_scenarios` where it also runs as a test. Surface syntax the
prototype compiler cannot parse yet (`X@task(...)`) grounds to its lowered
corpus spelling (`X_task(...)`) with a "future compiler lowering" comment at
the corpus site.

## Systemic gaps (design-level, fix once)

1. **`ai.run.Retry` / `ai.run.Fallback` (pages) vs `ai.retry()` / `ai.fallback()`
   (corpus).** The docs promise runner-wrapping; the corpus implements
   provider-wrapping. One of them must move.
2. **Named client values** (`FastModel`, `CarefulModel`, `TranscriptionModel`,
   `BackgroundModel`...). Pages assume `client`-style named declarations; the
   corpus builds providers with constructor calls (`live_openai()`). Either the
   scenarios grow named provider values or the pages switch to constructors.
3. **`ai.AgentOutcome<T>`** needs generic type aliases (compiler issue filed);
   corpus spells `Done<T> | BudgetReached | Handoff` inline. Pages now note this.
4. **Fixture identity drift**: each page invents its own functions
   (`DraftReply`, `ClassifyTicket`, `SummarizeCall`, `InspectClaim`,
   `AnswerPolicyQuestion`...). Scenarios are `ResolveTicket`-centric. Decide:
   port the page fixtures into scenarios (better coverage) or rewrite pages
   onto `ResolveTicket` (less work, more monotony). Recommend porting — each
   page's example becomes a scenario file under the matching guide number.

## Per-page ungrounded identifiers (audit 2026-07-28)

| Page | Ungrounded |
| --- | --- |
| README | `Help`, `Orders` |
| agents-and-tools | `AgentOutcome`, `OrderTools` |
| approvals-limits-and-handoffs | `Queue`, `Refund` |
| conversations-and-resuming | `CarefulModel`, `SupportModel` |
| dynamic-tools-and-mcp | `Convert`, `McpDiscovery` |
| harnesses-and-custom-extensions | `AuditResult`, `CodingModel`, `RepositoryReport`, `Summarize`, `WithAudit` |
| jobs-batches-and-caches | `AnswerPolicyQuestion`, `BackgroundModel`, `BatchModel`, `CachedModel`, `ClassifyTicket`, `DeepResolveTicket` |
| routing-retry-and-fallback | `CarefulModel`, `FastModel` |
| streaming-media-and-transcription | `CallSummary`, `ClaimEvidence(-Partial)`, `Explanation`, `InspectClaim`, `Receipt`, `Statement`, `SummarizeCall`, `TranscribeAudio`, `TranscriptionModel` |
| tasks-runners-and-results | `Draft`, `DraftReply` |
| testing-and-observability | `AgentOutcome`, `ConsoleObserver` |
| voice-and-live-sessions | `Help` |

Audit is mechanical (identifier extraction from ```baml fences, greps the
corpus); re-run after any page or scenario change.
