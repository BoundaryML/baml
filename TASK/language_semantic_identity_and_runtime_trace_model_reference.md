---
title: "BEP-053: Language Semantic Identity and Runtime Trace Model"
status: proposed
version: 2
created: 2026-05-21
updated: 2026-05-21
shepherds:
  - rossirpaulo
---
# BEP-053: Language Semantic Identity and Runtime Trace Model

## Summary

This BEP defines the product contract for semantic identity in the full BAML language. The goal is to let Studio, evals, rollouts, caches, and support workflows answer concrete questions about code changes:

- Did this function's callable contract change?
- Did its behavior change without changing its callable contract?
- Did it change directly, or only because a dependency changed?
- Are two production traces running the same effective version of the function?
- Did a type/schema change, a prompt change, a client change, a helper change, or only source/debug metadata change?

The hash model follows from those questions. BAML definitions expose four conceptual semantic version signals:

| Signal | Meaning | Example question |
|---|---|---|
| `direct_interface` | This definition's own callable/schema surface changed. | “Did `ExtractResume` itself change its params or return type?” |
| `effective_interface` | This definition's surface changed directly or through an interface dependency. | “Does this function still expose the same effective output schema?” |
| `direct_implementation` | This definition's own body/config/behavior changed. | “Did the prompt, default expression, body, or direct client reference change?” |
| `effective_implementation` | This definition's behavior changed directly or through an implementation dependency. | “Could this function behave differently for the same inputs?” |

These are **semantic lanes**. The implementation may store direct lanes and derive effective lanes from the dependency graph, or store all lanes for query speed. Product-wise, the four distinctions must exist.

This BEP is scoped to the current `baml_language` compiler/runtime and to Studio's product questions.

Additional pages:

- [Technical implementation direction](./pages/technical-implementation.md)
- [Scenario catalog](./pages/scenario-catalog.md)

## Motivation

BAML Studio receives traces from user code. A trace tells us what happened during one invocation: arguments, span tree, rendered prompt, provider calls, responses, output, errors, tags, logs, and timing. But trace data alone cannot answer whether two invocations used the same code.

Source text alone is also insufficient. A comment-only edit changes the source file but not the semantics. A helper function edit can change the behavior of `ExtractResume` even if the text of `ExtractResume` did not change. A class field type change can change the effective output schema of every LLM function returning that class. A client config change can alter behavior without touching any function body. A lambda has no user-written name, but its identity still matters if we want to explain callback or closure behavior.

The product need is therefore a **change-classification system**, not just a hash. We need enough identity information to group traces, compare evals, explain rollouts, invalidate caches, and tell users what changed.

## Design principle

This BEP treats hashes as opaque version signals. A user or Studio workflow should not need to know whether the implementation is a Merkle tree, graph hash, SCC hash, compiler-computed hash, or server-computed hash. The contract is that when code changes from A to B, Studio can or cannot answer a specific question.

For example, this is the right product statement:

> The return class `Resume` changed because one of its field types changed. Studio can tell that `ExtractResume`'s own signature did not directly change, but its effective interface did change through `Resume`.

This is the wrong starting point:

> `ExtractResume` has hash `abc123` and then hash `def456`.

The second statement is only useful after the first statement is true.

## Definitions

### Definition

A **definition** is a named or generated semantic unit whose version can matter to Studio. Examples:

- function
- method
- lambda definition
- class
- enum
- type alias
- client
- retry policy
- template string
- top-level `let`
- generated companion
- auto-derived function
- builtin/sysop

Anonymous things still need definition identity. A lambda can be identified by its enclosing definition plus a stable lexical path. A closure is not a definition; it is a runtime instance of a lambda plus captured values.

### Direct vs effective

A **direct** signal changes only when the definition's own declaration/body/config changes.

An **effective** signal changes when the definition changes directly or when any dependency relevant to that signal changes.

Example:

```baml
class Contact {
  email string
}

class Resume {
  contact Contact
}

function ExtractResume(doc: string) -> Resume {
  client GPT4
  prompt #"extract resume"#
}
```

If `Contact.email` changes from `string` to `string?`:

- `Contact.direct_interface` changes.
- `Resume.direct_interface` does not change; it still says `contact Contact`.
- `Resume.effective_interface` changes because `Contact` changed.
- `ExtractResume.direct_interface` does not change; it still returns `Resume`.
- `ExtractResume.effective_interface` changes because the returned schema changed through dependencies.

This is the distinction Studio needs for useful explanations.

### Interface vs implementation

**Interface** means the callable/schema surface:

- function name and identity
- parameter names, order, required/optional status, and types
- return type
- declared or callable throws/effect surface
- class fields
- enum variants
- type alias target
- function type/callback signatures
- schema/model/parser-visible type metadata, when such metadata exists in the BAML language

**Implementation** means behavior behind the surface:

- function body
- default expressions
- prompt text
- client reference
- client config
- retry behavior
- helper calls
- top-level `let` initializers
- template strings
- generated body recipe
- sysop/builtin semantic version

A change may affect both. For example, adding a required parameter changes the interface and usually changes implementation behavior because call binding changes.

### Source snapshot vs semantic identity

A **source snapshot** is textual. It changes when source paths or source contents change.

A **semantic identity** is resolved and typed. It should not change for comments, docstrings, formatting, spans, or source maps.

Source snapshots let Studio show the code. Semantic identity lets Studio group and compare behavior.

### Runtime trace identity

Runtime IDs identify one execution. They are separate from semantic IDs.

- `trace_id`: one root invocation tree.
- `span_id`: one span inside that tree.
- `event_id`: one event emitted during the trace.
- `definition_key`: the semantic definition being executed, if any.

Runtime values such as arguments, outputs, LLM responses, token usage, stream chunks, closure captured values, and host callback results never affect semantic hashes.

## What four lanes buy us

The four lanes are necessary because different user queries require different distinctions.

| Simpler model | What breaks |
|---|---|
| One project hash | Can only answer “something changed.” Cannot explain which functions/types/evals/traces are affected. |
| One hash per function | Cannot distinguish contract changes from behavior-only changes. |
| Direct hashes only | Cannot tell that `ExtractResume` is affected when its returned type `Resume` changes. |
| Effective hashes only | Cannot tell whether `ExtractResume` itself changed or only one of its dependencies changed. |
| Interface/implementation only, no direct/effective split | Cannot distinguish “the function's own signature changed” from “a nested output type changed.” |
| Direct/effective only, no interface/implementation split | Cannot distinguish “same schema, prompt changed” from “schema changed, prompt same.” |

The model is not complex for its own sake. Each extra lane preserves a product distinction users will ask about in Studio, evals, rollouts, and cache decisions.

## Which definitions need which lanes?

Every definition participates in the same conceptual model, but not every definition has meaningful data in every lane.

### Functions, methods, and lambdas

Functions generally need all four lanes:

- `direct_interface`: own signature/callable surface.
- `effective_interface`: own signature plus transitive type/callback interface dependencies.
- `direct_implementation`: own body/prompt/default expressions/direct config.
- `effective_implementation`: own body/config plus helpers, clients, retry policies, template strings, top-level lets, output parsing/schema dependencies, and generated/builtin versions.

### Classes, enums, and type aliases

Enums and type aliases are usually interface-only definitions:

- `direct_interface`: own variants or alias target.
- `effective_interface`: own definition plus transitive referenced types.

Classes are not merely structural in current BAML. A class has a schema surface and can also have a member API surface.

Class interface lanes should therefore include both:

- **Schema surface**: class name, generic params, fields, field types, and schema-semantic attributes.
- **Member API surface**: method set, method names, method signatures, and generated/companion members if they are exposed as part of the class API.

Changing a class field should change the class schema interface. Adding, removing, renaming, or changing the signature of a method should change the class member interface. Changing only a method body should not make the class schema look changed; the method body is hashed as that method definition's implementation.

Implementation lanes are absent for enums/type aliases and for class schema itself unless the language introduces class-level behavior that is not represented as a method. Do not invent implementation lanes for symmetry, and do not hide method signatures by treating classes as purely structural.

### Clients, retry policies, template strings, and top-level lets

These are usually implementation dependencies of functions:

- a client config change should affect functions that use that client through `effective_implementation`;
- a retry policy change should affect functions that use that policy through `effective_implementation`;
- a template string change should affect prompt-building functions through `effective_implementation`;
- a top-level `let` initializer change should affect dependents through `effective_implementation`.

If a future feature makes any of these part of a public callable surface, that feature must state which interface lane it affects.

## What counts and what does not

### Comments and docstrings

Comments and docstrings are documentation. They must not affect semantic lanes.

They do affect the source snapshot, because Studio should still show the exact source the user wrote. But a docstring-only edit should not create a new function/type semantic version.

### Attributes and decorators

The BAML language currently carries raw attributes in the AST and type attributes in the type system. The product rule is:

```text
Hash an attribute only if the current BAML language gives it semantic behavior.
Do not hash raw attribute syntax merely because it appears in the AST.
Do not hash attribute source spans.
```

More concretely:

- If an attribute changes parsing, streaming, schema generation, type checking, or runtime behavior, it belongs in the appropriate semantic lane.
- If an attribute is ignored by lowering/runtime today, it does not belong in semantic identity today.
- If an attribute exists only for diagnostics, source mapping, editor behavior, visualization, or documentation, it belongs outside semantic lanes.
- If a low-level crate still has a placeholder for an older or not-yet-productized feature, that does not make it a semantic product requirement until the current BAML language consumes it.

This avoids importing stale assumptions while still leaving room for current or future BAML-language semantic attributes.

### Headers, block annotations, watch metadata, and visualization metadata

Header comments and visualization/debug annotations should not change `direct_implementation` unless the language explicitly defines them as runtime behavior.

If Studio needs to detect them, use a separate debug/source metadata identity:

```text
source_snapshot_id changed
semantic lanes unchanged
debug_metadata_version changed
```

This lets Studio answer “only the visualization changed” without claiming the function behavior changed.

### Source spans, source maps, local IDs, and object indexes

These never belong in semantic lanes:

- source spans
- line/column ranges
- source-map entries
- local slot numbers
- object pool indexes
- bytecode PCs
- arena/local item IDs
- pointer addresses
- `HashMap` iteration order

They may appear in uploaded source/debug metadata for navigation, but not as semantic identity.

### Runtime values

The following are trace data only and never semantic identity:

- function arguments
- outputs
- errors
- LLM responses
- provider usage
- stream chunks
- closure captured values
- host callback results
- environment variable values

Environment variable **names/config paths** may appear in implementation identity if the code/config references them. Environment variable **values** must not.

## Product queries this unlocks

| User or Studio question | Required signal |
|---|---|
| “Show me all production traces for the same version of `ExtractResume`.” | definition key + `effective_implementation` |
| “Did this eval compare the same callable contract?” | `effective_interface` |
| “Did the function body/prompt itself change?” | `direct_implementation` |
| “Did the output schema change because a returned type changed?” | `effective_interface` plus dependency edges |
| “Can we safely reuse cached outputs?” | Usually `effective_implementation`, plus runtime/cache policy |
| “Can generated clients stay the same?” | `effective_interface` for generated surfaces |
| “Did this failure come from a function edit or dependency edit?” | Direct vs effective diff |
| “Did only source formatting/comments/docstrings change?” | `source_snapshot_id` changed, semantic lanes unchanged |
| “Did only Studio/debug visualization metadata change?” | debug/source metadata changed, semantic lanes unchanged |
| “Which functions are affected by this type change?” | reverse dependency graph over `effective_interface` |
| “Which functions are affected by this client change?” | reverse dependency graph over `effective_implementation` |
| “Can I compare eval results before/after a prompt change while holding schema constant?” | same `effective_interface`, different `direct_implementation` |

## Change examples

This section is the product contract. The [scenario catalog](./pages/scenario-catalog.md) contains additional edge cases.

### 1. Comment or docstring-only edit

Before:

```baml
/// Extracts a resume.
function ExtractResume(doc: string) -> Resume {
  client GPT4
  prompt #"extract resume"#
}
```

After:

```baml
/// Extracts a structured resume from plain text.
function ExtractResume(doc: string) -> Resume {
  client GPT4
  prompt #"extract resume"#
}
```

Expected:

| Signal | Changes? |
|---|---:|
| source snapshot | yes |
| debug/source metadata | maybe |
| semantic lanes | no |

Studio can say: “The source text changed, but the callable contract and behavior identity did not.”

### 2. Required parameter added

```baml
function ExtractResume(doc: string) -> Resume { ... }
```

becomes:

```baml
function ExtractResume(doc: string, locale: string) -> Resume { ... }
```

Expected:

| Signal | Changes? |
|---|---:|
| `direct_interface` | yes |
| `effective_interface` | yes |
| `direct_implementation` | likely yes |
| `effective_implementation` | yes |

Studio can say: “The function's own callable contract changed.”

### 3. Default expression changed

```baml
function Search(query: string, limit: int = 10) -> Results { ... }
```

becomes:

```baml
function Search(query: string, limit: int = 20) -> Results { ... }
```

Expected:

| Signal | Changes? |
|---|---:|
| `direct_interface` | no |
| `effective_interface` | no, unless dependencies changed |
| `direct_implementation` | yes |
| `effective_implementation` | yes |

Studio can say: “The function remains callable the same way, but default behavior changed.”

### 4. Default presence changed

```baml
function Search(query: string, limit: int) -> Results { ... }
```

becomes:

```baml
function Search(query: string, limit: int = 10) -> Results { ... }
```

Expected:

| Signal | Changes? |
|---|---:|
| `direct_interface` | yes |
| `effective_interface` | yes |
| `direct_implementation` | yes |
| `effective_implementation` | yes |

Studio can say: “The call surface changed because callers may now omit `limit`.”

### 5. Returned type changed through dependency

Before:

```baml
class Contact {
  email string
}

class Resume {
  contact Contact
}

function ExtractResume(doc: string) -> Resume { ... }
```

After:

```baml
class Contact {
  email string?
}

class Resume {
  contact Contact
}

function ExtractResume(doc: string) -> Resume { ... }
```

Expected:

| Definition | Direct interface | Effective interface |
|---|---:|---:|
| `Contact` | changes | changes |
| `Resume` | unchanged | changes |
| `ExtractResume` | unchanged | changes |

A simpler two-hash model breaks here. If it stores only a direct function signature hash, it misses that `ExtractResume`'s output schema changed. If it stores only a full/effective signature hash, it cannot explain that the text of `ExtractResume` did not change.

### 6. Helper function body changed

Before:

```baml
function NormalizeText(s: string) -> string {
  return s.trim()
}

function ExtractResume(doc: string) -> Resume {
  let clean = NormalizeText(doc)
  client GPT4
  prompt #"extract from {{ clean }}"#
}
```

After:

```baml
function NormalizeText(s: string) -> string {
  return s.trim().lowercase()
}
```

Expected:

| Definition | Direct implementation | Effective implementation |
|---|---:|---:|
| `NormalizeText` | changes | changes |
| `ExtractResume` | unchanged | changes |

Studio can say: “`ExtractResume` may behave differently because helper `NormalizeText` changed; `ExtractResume` itself did not change.”

### 7. Prompt text changed

```baml
prompt #"extract resume"#
```

becomes:

```baml
prompt #"extract resume and include missing fields as null"#
```

Expected:

| Signal | Changes? |
|---|---:|
| `direct_interface` | no |
| `effective_interface` | no, unless schema deps changed |
| `direct_implementation` | yes |
| `effective_implementation` | yes |

Studio can group this as a behavior/prompt change while holding schema constant.

### 8. Client config changed through dependency

```baml
client GPT4 {
  provider "openai"
  model "gpt-4.1"
}

function ExtractResume(doc: string) -> Resume {
  client GPT4
  prompt #"extract resume"#
}
```

becomes:

```baml
client GPT4 {
  provider "openai"
  model "gpt-4.2"
}
```

Expected:

| Definition | Direct implementation | Effective implementation |
|---|---:|---:|
| `GPT4` | changes | changes |
| `ExtractResume` | unchanged | changes |

Studio can say: “The function text did not change, but its effective implementation changed because the client changed.”

### 9. Package dependency exported type changed

Package `shared` before:

```baml
class Address {
  city string
}
```

Package `shared` after:

```baml
class Address {
  city string
  country string?
}
```

User package:

```baml
function ExtractAddress(doc: string) -> shared.Address { ... }
```

Expected:

- We should not hash the dependency package's raw AST into the user package.
- We should include the referenced exported semantic fingerprint for `shared.Address`.
- `ExtractAddress.effective_interface` changes because the exported type contract changed.

This is an open implementation area because exported semantic fingerprints are not yet a fully specified language artifact. The product direction is clear: package boundaries should hide raw AST while still propagating exported semantic changes.

### 10. Lambda body changed

```baml
function MapNames(names: string[]) -> string[] {
  return names.map((name) => name.trim())
}
```

becomes:

```baml
function MapNames(names: string[]) -> string[] {
  return names.map((name) => name.trim().lowercase())
}
```

Expected:

| Signal | Changes? |
|---|---:|
| parent `direct_interface` | no |
| parent `direct_implementation` | yes |
| lambda definition identity | changes |

Studio should be able to explain that the implementation changed inside a lambda, even though the lambda has no user-written top-level name.

### 11. Closure captured value changed at runtime

Same code:

```baml
function MakePrefixer(prefix: string) -> (string) -> string {
  return (value) => prefix + value
}
```

Run A captures `"dev-"`; run B captures `"prod-"`.

Expected:

| Thing | Changes? |
|---|---:|
| lambda definition semantic identity | no |
| closure runtime instance | yes |
| trace payload | yes |

Captured runtime values explain a trace. They do not change code identity.

### 12. Callback implementation changed outside BAML

```baml
function Transform(input: string, callback: (string) -> string) -> string {
  return callback(input)
}
```

If the host-provided callback implementation changes but the BAML source and callback type do not:

- `Transform` semantic lanes do not change.
- The runtime trace should identify the callback call as an external callback if the bridge can provide stable metadata.
- Whether Studio can group host callback versions is an open bridge contract question, not a BAML semantic hash question.

### 13. Header/debug metadata changed

```baml
//# Extraction
function ExtractResume(doc: string) -> Resume { ... }
```

becomes:

```baml
//# Resume extraction
function ExtractResume(doc: string) -> Resume { ... }
```

Expected:

| Signal | Changes? |
|---|---:|
| source snapshot | yes |
| debug metadata | yes, if Studio tracks it |
| semantic lanes | no |

Studio can say: “Only visualization/debug metadata changed.”

## Runtime trace relationship

Semantic identity answers “which code version was this?” Runtime trace identity answers “what happened during this invocation?”

A function trace event should carry enough information to join back to semantic identity:

```text
trace_event {
  trace_id,
  span_id,
  event_id,
  source_snapshot_id,
  definition_key,
  runtime payload...
}
```

The runtime should not recompute semantic hashes on the hot path. It should emit stable definition keys and source snapshot references. The canonical hash computation location is an implementation decision discussed below.

## Canonical implementation stance

### Durable hash algorithm

Do not use Rust `DefaultHasher`/`u64` as the Studio-facing identity contract.

A `u64` hash may be fast and familiar, but this is a durable product identity stored across deployments, runtimes, compiler versions, and backend queries. The canonical identity should use:

```text
versioned canonical serialization + explicit full-width hash algorithm
```

This BEP recommends **BLAKE3-256** for performance and full-width identity. SHA-256 is acceptable if backend/platform conventions prefer it. The exact algorithm can be finalized in implementation, but `DefaultHasher`/`u64` should not be the durable contract.

### One canonical computation path

Avoid dual implementations. The system should have one canonical place where semantic fingerprints are computed.

Open question:

- **Server-side canonical hashing**: Studio already receives source snapshots and source maps. The cloud computes semantic fingerprints from the uploaded source snapshot. This minimizes runtime/compiler complexity for the Studio use case.
- **Compiler-side canonical hashing**: the Rust compiler computes fingerprints during compilation. This is better for local codegen cache invalidation or offline tools, but it couples the hash product to compiler plumbing and requires the runtime/upload path to carry more precomputed data.

Recommendation for v1: **server-side canonical hashing for Studio version queries**, because this BEP is primarily about Studio traces and source snapshots are already required. If local codegen cache invalidation becomes a separate product requirement, add compiler-side computation later using the same canonical schema. Do not build two independent hash implementations now.

### Dependency graph, not source text

Effective lanes should be computed from typed dependency edges, not from textual includes or raw AST traversal.

Required dependency classes include:

- function call dependencies
- method call dependencies
- callback/function type dependencies
- type dependencies
- template string dependencies
- client dependencies
- retry policy dependencies
- top-level `let` dependencies
- generated-from dependencies
- builtin/sysop semantic version dependencies
- package export dependencies

The implementation may use SCC/graph analysis to compute effective lanes, but the product contract is dependency-driven change explanation.

## Package dependencies

Packages are semantic boundaries.

A package should not hash another package's raw AST into its own identity. Instead, cross-package edges should point to the dependency package's exported semantic fingerprints.

This gives us both properties we want:

- package internals can change without affecting dependents when exported contracts do not change;
- exported type/function/client changes still propagate to dependents that use them.

Open question:

```text
What exact artifact represents an exported package fingerprint today?
```

The current direction is to derive it from the resolved package interface: exported types, exported functions, and any exported config surfaces that other packages can reference.

## Anonymous functions, lambdas, closures, and callbacks

### Lambda definitions

A lambda is code and should have semantic identity even though it has no user-written global name.

Recommended identity:

```text
lambda_definition_key = enclosing_definition_key + stable_lexical_lambda_path
```

The semantic lanes for the parent function should include lambda changes as part of the parent's implementation. Studio may additionally expose the lambda definition for detailed debugging.

### Closure instances

A closure is a runtime instance of a lambda plus captured values.

Captured values are trace data only. They must not change the lambda's semantic identity.

### Host callbacks

Host callbacks are not BAML definitions unless the bridge supplies stable callback metadata. The BAML function type/interface can be hashed. The host callback implementation version is a bridge contract question.

## What this BEP intentionally leaves open

- Exact canonical serialization format.
- Final hash algorithm between BLAKE3-256 and SHA-256.
- Whether server-side hashing remains sufficient once local cache invalidation becomes product scope.
- Exact package export fingerprint schema.
- Exact stable lexical path format for lambdas and generated definitions.
- Bridge contract for host callback version identity.
- Whether low-level type assertion placeholders still represent a current BAML language feature. Until verified as current language semantics, they are not product examples in this BEP.

## Acceptance criteria

The BEP is satisfied when Studio can distinguish the following changes without manually diffing source text:

- comment/docstring-only edit;
- function signature edit;
- default expression edit;
- default presence edit;
- prompt edit;
- helper body edit;
- returned type dependency edit;
- nested type dependency edit;
- client config edit;
- retry policy edit;
- template string edit;
- top-level `let` edit;
- lambda body edit;
- closure captured runtime value change;
- package exported type/function edit;
- header/debug metadata edit.

For each scenario, Studio should be able to say whether the changed signal was direct or dependency-derived, and whether it affected interface, implementation, source/debug metadata, runtime trace data, or none of the above.

## Test plan

Golden tests should be scenario-first. Each test should have code A, code B, and the expected changed lanes.

Minimum tests:

1. Comment/docstring edit changes source snapshot only.
2. Formatting edit changes source snapshot only.
3. Required parameter added changes direct/effective interface.
4. Default value expression edit changes direct/effective implementation only.
5. Default presence edit changes interface and implementation.
6. Return type name unchanged but nested field type changed changes effective interface only for dependents.
7. Helper body edit changes caller effective implementation but not caller direct implementation.
8. Prompt edit changes direct implementation.
9. Client config edit changes dependent effective implementation.
10. Retry policy edit changes dependent effective implementation.
11. Top-level `let` initializer edit changes dependent effective implementation.
12. Lambda body edit changes parent direct implementation and lambda identity.
13. Captured closure value edit across runs changes trace data only.
14. Header/debug metadata edit changes debug metadata only.
15. Cross-package exported type edit changes dependent effective interface without hashing raw dependency AST.
16. Reordering maps/files does not change semantic identity.
17. Local numeric ID/object index changes do not change semantic identity.


---

## Additional Pages

- [Scenario Catalog](./pages/scenario-catalog.md)
- [Technical Implementation Direction](./pages/technical-implementation.md)


---

<!-- block-comments -->

<!-- @discussion by hellovai | "
function body
default expressions
prompt text
client refere..." -->
> > client config
>
> technically clients are top level lets :')
<!-- /@discussion -->

<!-- @discussion by hellovai | "
trace_id: one root invocation tree.
span_id: one span insid..." -->
> > trace_id: one root invocation tree. span_id: one span inside that tree.
>
> why do we need trace_id vs span_id?
<!-- /@discussion -->

<!-- @discussion by hellovai | "
direct_interface: own signature/callable surface.
effective..." -->
> > interface
>
> i would qualify its transitive type `effective_interface`
<!-- /@discussion -->

<!-- @discussion by hellovai | "Classes, enums, and type aliases" -->
> > Classes
>
> my one worry here is type_builder.
>
> v1 hashes (the one in engine) didn't support this and it required a lot of work to do it so we abandoned it
<!-- /@discussion -->

<!-- @discussion by hellovai | "
a client config change should affect functions that use tha..." -->
> > a client config change should affect functions that use that client through effective_implementation;
>
> if `client` option on a function becomes an expression, this may be harder to tell.
<!-- /@discussion -->

<!-- @discussion by hellovai | "SignalChanges?direct_interfaceyeseffective_interfaceyesdirec..." -->
> > direct_implementation	likely yes
>
> we can say {x}_implement hash depends on {x}_interface hash
<!-- /@discussion -->

<!-- @discussion by hellovai | "Studio can say: “The function remains callable the same way,..." -->
> > Studio can say: “The function remains callable the same way, but default behavior changed.”
>
> i really love these sentences. they help a lot!!
<!-- /@discussion -->

<!-- /block-comments -->

---

## Additional Pages

- [Scenario Catalog](pages/scenario-catalog.md)
- [Technical Implementation Direction](pages/technical-implementation.md)
