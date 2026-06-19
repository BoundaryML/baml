# Scenario 07 — Reasoning models (effort, verbosity, thinking, continuity)

This scenario stresses the Provider/capability model against reasoning models, where the hard parts are: reasoning is a *distinct content kind* (not text, so it must be displayed/billed/stripped without contaminating the answer `T`); two incompatible control models (OpenAI's categorical `effort` vs Anthropic/Gemini's token *budget*) plus a separate `verbosity` knob; and lossless cross-turn *continuity* of signed/encrypted thinking — including OpenAI's server-held state. `implement.baml` adds a `Thinks` companion (out-of-band reasoning via `WithReasoning<T>`) and a `Continuity` capability with an opaque, provider-owned `ContinuationState` (the `Tools.Transcript` pattern); `usage.baml` shows the ergonomics and where portability leaks; `evaluation.md` is adversarial about the server-state and Fallback-vs-continuity failure modes.

Background: background/01-single-turn.md → ## ◆ Reasoning models
