# BAML 0.25.0 release audit

Prepared on 2026-11-05 Pacific time for the 2026-11-06 UTC release. All inventories, effects, examples, and follow-ups use these fixed boundaries.

- Lower: published `baml-language-0.24.0`, `dec4713a0e7c9699442bf5855b4f327063f769c5`.
- Upper: 0.25.0 release source, `a2f6d5403f399b0b3cc1a0902b0b3dd208c6043a`, selected by [release run 42150548387](https://github.com/BoundaryML/baml/actions/runs/42150548387). The tag was not yet created when these references were pinned. Verify its final target before publication.
- The range contains 14 squash-merged PRs across the whole repository. All are v1, including website, workflow, tests, and metadata PRs excluded from the public post.
- The 0.24.0 registry release did not promote the canary channel. #5551 explicitly replaces it with 0.25.0 instead of rerunning the old cut. The new post links the 0.24.0 migrations for users coming from the last promoted 0.23.0 canary.
- The existing changelog branch was rebased onto this upper source. The 0.24.0 notes and audits remain separate. This directory records the additional 0.25.0 inventory without overwriting the earlier audit.

The body below is the reviewed aggregation. Each feature combines at most two PRs; each bug fix cites one.

# Features

## Choose the generated SDK's package name

Set `package_name` in a generator to choose its output package and directory. The default remains `baml_sdk`.

The examples below generate bindings for this function:

```baml
function Identity(value: string) -> string { value }
```

### Python

Use a dotted import path to keep a library's generated code inside its own package:

```toml
[generator.python]
output_type = "python/pydantic"
naming_convention = "language"
output_dir = "src"
package_name = "my_app._generated"
```

This writes `src/my_app/_generated`. BAML creates missing parent directories and leaves `my_app/__init__.py` alone. Generated imports and type-map lookups follow the package's actual location.

After regenerating, change imports such as:

```python
from baml_sdk import identity
```

To:

```python
from my_app._generated import identity

result = await identity("hello")
```

This prevents two installed Python libraries from overwriting the same top-level `baml_sdk` package. It does not provide independent runtimes: one process still holds one BAML program and type map. Generated SDKs also retain exact bridge-version dependencies. Do not assume that renaming permits two independently loaded BAML programs or different bridge versions in one process.

### Go, Rust, and TypeScript

Go uses the configured name for its root package and directory. When `package_name` is set, the last segment of `sdk_import_path` must match it:

```toml
[generator.go]
output_type = "go"
naming_convention = "language"
output_dir = "generated"
package_name = "appapi"
sdk_import_path = "example.com/my-app/generated/appapi"
```

Update imports and package qualifiers to the generated name:

```go
import "example.com/my-app/generated/appapi"

// In a function with a context.Context named ctx:
value, err := appapi.Identity(ctx, "hello")
```

Rust uses the name for the directory, Cargo package, and library target:

```toml
[generator.rust]
output_type = "rust"
naming_convention = "language"
output_dir = "generated"
package_name = "app_api"
```

Update Cargo path dependencies and `use` paths when opting in:

```toml
[dependencies]
app_api = { path = "generated/app_api" }
```

```rust
use app_api::identity;

// In an async function:
let value = identity("hello".to_owned()).await?;
```

TypeScript uses `package_name` for the generated directory. It does not set an npm package name:

```toml
[generator.typescript]
output_type = "typescript/node"
naming_convention = "preserve-case"
output_dir = "src"
package_name = "generated-api"
```

Update relative imports to that directory:

```typescript
import { Identity } from "./generated-api/index.js";

const value = await Identity("hello");
```

The option also supports the TypeScript web generator. Swift, Java, C++, and C# reject it for now. Names must satisfy the target language's rules and work as portable directory names.

### Regeneration and existing files

Use the 0.25.0 CLI for this setting. Older CLIs can warn about the unknown key and still write `baml_sdk`.

When `package_name` is explicit, generation refuses a nonempty destination that is not owned by `baml generate`. Choose an empty directory or a new name. BAML checks named destinations before writing any generator output.

If the rename leaves an old generated `baml_sdk` directory behind, the CLI warns and leaves it in place. Update imports, build paths, and packaging rules, then remove the obsolete generated directory after checking that nothing still uses it. Python regeneration can change internal module-path bookkeeping even when the default package name stays the same.

([#5532](https://github.com/BoundaryML/baml/pull/5532)), ([#5533](https://github.com/BoundaryML/baml/pull/5533))

## Read redacted values in telemetry snapshots

The telemetry reader accepts BTELCAS format 6 recordings containing redacted values. CLI queries and the playground distinguish these from null, missing, and unavailable values.

```bash
baml query --local "SELECT baml_value_state(output_value) AS state, baml_kind(output_value) AS kind FROM spans"
```

For a redacted value, both functions report `redacted`. Navigating inside that value preserves the redacted state. Rendered values use a `$redacted` envelope. Comparisons that depend on hidden data return an unknown result; a visible difference elsewhere can still establish inequality.

This is reader support only. BAML still writes format 5 recordings. This release does not add a way to mark application values for redaction.

([#5543](https://github.com/BoundaryML/baml/pull/5543))

# Bug fixes

- Removed unreachable fallback statements in the embedded standard library, so `baml check` no longer reports those warnings in otherwise clean projects. User-code unreachable statements are still diagnosed. ([#5501](https://github.com/BoundaryML/baml/pull/5501))
- Corrected `baml describe cleanup` and `baml describe catch_all` examples to show actual `pattern => handler` arms. ([#5547](https://github.com/BoundaryML/baml/pull/5547))
- Fixed the Homebrew publishing workflow's trust check before auditing the wrapper formula. This addresses the publishing failure that prevented 0.24.0 canary promotion. ([#5546](https://github.com/BoundaryML/baml/pull/5546))
