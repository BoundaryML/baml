# agent-bench 20260714-143253

Question: What kinds of errors can root.cc.Parser.parse_stmt throw, and where do they originate?

| run | wall (s) | calls | BAML calls | grep/read calls | CLI output bytes | final bytes | tokens |
|---|---|---|---|---|---|---|---|
| claude/guided-subagent r1 | 31 | 4 | 5 | 0 | ? | ? | turns=2 cost=$0.14946084999999998 model=claude-haiku-4-5-20251001,claude-opus-4-8[1m],claude-sonnet-5 |

## claude/guided-subagent r1 — commands
```
Agent	Navigate parse_stmt errors
Bash	baml describe root.cc.Parser.parse_stmt --view source --output compact
Bash	baml describe root.cc.Parser.parse_stmt --view source --max-lines 200 --output compact
Bash	baml describe root.cc.Parser.eat --view source --output compact
baml describe root.cc.Parser.parse_expr --view source --output compact
baml describe root.cc.Parser.parse_block --view source --output compact
```

## claude/guided-subagent r1 — final answer

`root.cc.Parser.parse_stmt` can throw exactly one error type: **`CError`**.

**Error kind**
- `CError` is the only throwable type — declared at `baml_src/ns_cc/errors.baml:3`. `parse_stmt` is annotated `throws CError` at `baml_src/ns_cc/parser.baml:147`.

**Where it originates**

`parse_stmt` (`baml_src/ns_cc/parser.baml:147-186`) has no `throw` of its own — it propagates `CError` from its callees:

- **`self.eat(...)`** — the concrete throw site: `throw CError { message: "syntax error: unexpected '" + t.text + "'" }` at `baml_src/ns_cc/parser.baml:19` (in function `eat`, lines 16-23). `parse_stmt` calls `eat` for `Semi`, `Ident`, `Assign`, `LParen`, `RParen` tokens (lines 152, 157, 158, 165, 167).
- **`self.parse_block()`** (`parser.baml:137-145`) — calls `self.eat(LBrace)`/`self.eat(RBrace)` (same throw site, line 19) and recurses into `parse_stmt`.
- **`self.parse_expr()`** (`parser.baml:133-135`) — doesn't throw directly; it delegates to `self.parse_assign()` and re-raises any `CError` originating deeper in that chain.

So the one provable throw site is `parser.baml:19` inside `eat`. The navigator noted that `parse_expr`'s deeper origin (inside `parse_assign` and below) wasn't traced, since it fell outside the directly-inspected call chain — so I can't confirm additional throw sites there without further investigation.

