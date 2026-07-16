# Scenario 10 — The agentic loop + stop conditions

This scenario shows the proposed model carrying a real agent runner: the multi-turn tool loop is the `Tools.run_tools` *default method*, so the trivial case is free; stop policy (a numeric step cap, a `stopWhen` predicate, ORed predicates) lives in a `Bounded` combinator that *overrides only `run_tools`* and never touches the wire threading; the budget hit returns a best-effort partial (`Budget<T>`, the `Item | Done` shape one level up from `step`); and per-turn tool filtering plus assistant-turn `phase` preservation are exercised. `implement.baml` is the library code (two providers — OpenAI id-correlated, Gemini name+position — plus the `Bounded` policy combinator); `usage.baml` is the app author's `client` blocks and functions; `evaluation.md` stress-tests where it leaks.

Background: background/02-tools-and-agents.md → ## 2. ★ The tool loop / agentic loop
