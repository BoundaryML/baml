# cargo-stow specification

Status: implemented by the Rust crate in this directory, which is the reference
implementation. `tests/cases` holds the language-neutral tests.

## 1. Purpose

Keep a Rust workspace's structure legible and enforced:

1. Layering: which crates may depend on what.
2. Naming: a crate's name states its namespace and role.
3. Hygiene: one consistent way to declare dependencies.
4. A picture: a readable dependency graph.

## 2. Process

1. The Rust implementation follows this spec. It is the reference. (Done.)
2. Language-neutral tests are generated from it and reviewed (section 11). (Done.)
3. Rewrite in BAML against those tests.

## 3. Command line

| Command | Effect |
|---|---|
| `cargo stow` | Validate (section 7). |
| `cargo stow --fix` | Fix (section 8), then validate. |
| `cargo stow --graph FILE [--include-tests]` | Write the graph (section 9). Does not validate. |
| `cargo stow init [--minimal]` | Write a template `stow.toml` at the workspace root. Error if one exists. |
| `--verbose` | Also print each crate name as it is checked. |

There is no `--check` flag. `--fix` and `--graph` are mutually exclusive, and
`--include-tests` requires `--graph`.

Standard output, besides the validation report (7.3):

- `--fix` prints `fixed <path>` for each manifest it rewrites, in crate name order.
- `--verbose` prints `checking <crate>` for each crate, in name order.
- `init` prints `created stow.toml`.
- `--graph` prints nothing.

Exit codes: `0` success, `1` validation errors or any failure in section 10, `2` usage error.

## 4. Inputs

**Workspace.** Take the nearest `Cargo.toml` at or above the current directory. The
workspace is the one Cargo resolves for it, and the crates are its members, as reported by
`cargo metadata --no-deps`. Dependencies are never resolved or downloaded.

**Config.** `stow.toml` at the workspace root. If absent, `[workspace.metadata.stow]` in the
root `Cargo.toml`. If both exist, `stow.toml` wins. If neither exists, fail.

**Ignored crates.** A crate whose package name matches any `ignore_crates` glob is excluded
from every check, from `--fix`, and from the graph.

**Globs.** `*` matches any run of characters, including none. No other metacharacters.

## 5. Config schema

Top level:

| Key | Type | Meaning |
|---|---|---|
| `namespaces` | array of namespace | See below. |
| `dependency_rules` | array of rule | Rules applied to every crate. |
| `ignore_crates` | array of glob | Crates to ignore. |
| `graph.external_deps` | array of string | External packages shown in the graph. |
| `graph.implied_deps` | map string -> array of string | See 9.2. |

Namespace:

| Key | Type | Meaning |
|---|---|---|
| `name` | string | Prefix. Crates named `<name>_...` belong to it. |
| `approved_prefixes` | array of string | Words allowed as the first of two or three name parts. |
| `approved_suffixes` | array of string | Whole tails accepted verbatim. |
| `test_crate_exceptions` | array of string | Test crates that need no base crate. |
| `name_exceptions` | map folder -> package | Folders whose package name differs, or does not follow the convention. |
| `dependency_rules` | array of rule | Rules applied to crates in this namespace. |
| `link_crates` | array of string | See 9.4. |
| `embed_crates` | array of string | See 9.4. |

Rule:

| Key | Type | Meaning |
|---|---|---|
| `pattern` | glob, or `{ select = glob, exclude = [glob] }` | Which dependency names the rule restricts. |
| `allowed_crates` | array of glob | Crates that may use them. |
| `allowed_prefixes` | array of string | Allows `<ns>_<prefix>` and `<ns>_<prefix>_*` for every namespace `ns`. |
| `regular_deps_only` | bool, default false | Apply only to `dependencies` tables, not dev or build. |
| `reason` | string | Appended to the error. |

All keys are optional except `name` and `pattern`. Unknown keys are rejected, which
includes the old flat form with top-level `approved_prefixes`.

## 6. Definitions

**Namespace of a crate.** Among namespaces whose `<name>_` is a prefix of the package name,
the one with the longest name. Otherwise, a namespace whose `name_exceptions` has the
package name as a value. Otherwise none.

**Internal dependency.** A dependency whose name has a namespace. All others are external.

**Test crate.** A crate whose name ends in `_test` or `_tests`.

**Dependency tables.** `dependencies`, `dev-dependencies`, `build-dependencies`, and those
same three under every `target.<cfg>`. Every check and `--fix` cover all of them. A table is
"regular" if it is `dependencies` or `target.<cfg>.dependencies`.

**Dependency name.** The key of the entry in its table.

## 7. Validation

Each check yields zero or more errors. Any error means exit 1.

| Id | Rule |
|---|---|
| `nested-crate` | A crate directory is its member root or a direct child of it. A member root is the directory part of a `members` entry before its first wildcard segment; an entry with no wildcard is its own root. When several roots contain a crate, the longest applies. |
| `name-mismatch` | Package name equals folder name, unless some namespace has `name_exceptions[folder] == package`. A package at the workspace root is exempt. |
| `naming` | See 7.1. |
| `orphan-test-crate` | A test crate `X_test` or `X_tests` requires a non-ignored workspace crate `X`, unless listed in its namespace's `test_crate_exceptions`. |
| `non-workspace-dep` | Every dependency entry has `workspace = true`. |
| `restricted-dep` | See 7.2. |
| `unsorted-deps` | Within each dependency table: internal dependencies first, then external; each group ordered by lowercased name, ties by byte order. |
| `unused-workspace-dep` | Every key in `[workspace.dependencies]` is a dependency name in some non-ignored crate, or is the package name of a workspace member. |

### 7.1 Naming

Skipped if the folder has a matching `name_exceptions` entry. Otherwise the package name
must be `<namespace>_<tail>` for a configured namespace, and `tail` must satisfy one of:

1. `tail` is listed in `approved_suffixes`.
2. One part: one or more lowercase ASCII letters, or an approved prefix.
3. Two parts `a_b`: `a` is an approved prefix, or `b` is `types`, `test` or `tests`.
4. Three parts `a_b_c`: `a` is an approved prefix and `c` is `types`, `test` or `tests`.

Parts are separated by `_`. Four or more parts fail unless rule 1 applies.

### 7.2 Dependency restrictions

For a crate, the applicable rules are the top-level rules plus the rules of its namespace.
For each dependency name `d` in each dependency table `t`, and each applicable rule `r`:

- If `r.regular_deps_only` and `t` is not regular, skip.
- If `d` does not match `r.pattern`, skip. A `{ select, exclude }` pattern matches when `d`
  matches `select` and no `exclude` glob.
- The crate is allowed if it is a test crate, or matches an `allowed_crates` glob, or equals
  `<ns>_<p>` or starts with `<ns>_<p>_` for some namespace `ns` and allowed prefix `p`.
- If not allowed, report `restricted-dep` with `r.reason`.

A rule with no allow entries denies every non-test crate.

### 7.3 Output

One line per error on stdout, sorted by crate name, then by check order in the table above,
then by table name, then by dependency name:

```
error[<id>]: <crate>: <path to Cargo.toml>: <message>
```

Paths are relative to the workspace root. Workspace-level errors use `workspace` as the
crate. The last line is `ok` or `<n> error(s)`. Output is ASCII only.

## 8. Fix

`--fix` edits crate manifests, then runs validation. It only repairs `non-workspace-dep`
and `unsorted-deps`.

1. **Convert.** An entry that lacks `workspace = true` and whose name is a key in
   `[workspace.dependencies]` is rewritten to workspace form: `version`, `path`, `git`,
   `branch`, `tag`, `rev` and `registry` are removed and `workspace = true` is added,
   first in an inline entry and last in a sub-table entry. `features`, `optional` and
   `default-features` are kept, in their original order. An entry with any other key is
   not converted and stays an error.
2. **Sort.** Each dependency table is reordered to satisfy `unsorted-deps`.

Fix is lossless:

- An entry moves together with the comment lines directly above it (no blank line between)
  and any comment on its own line end.
- A comment followed by a blank line is free-standing and keeps its position in the table.
- Nothing else in the file changes: no reformatting, no key reordering inside an entry.
- A file is written only if its content changes. Running `--fix` twice changes nothing the
  second time.
- A table containing an entry in sub-table form (`[dependencies.foo]`) or dotted form
  (`foo.workspace = true`) is not reordered; it stays an error if unsorted.

Stow does not format manifests.

## 9. Graph

`--graph FILE` writes Mermaid flowchart text to `FILE`.

### 9.1 Nodes and edges

- Crate nodes: non-ignored workspace members that have a namespace.
- External packages: those listed in `graph.external_deps`. They are leaves; edges never
  start from them.
- Edges: for each crate node, each dependency declared in a regular table (plain or
  `target.<cfg>`) whose package is another crate node or a listed external package.
  Dev and build dependencies are not edges.
- The package of a dependency is the `package` value of its `[workspace.dependencies]`
  entry if there is one, otherwise its name.

### 9.2 Edge removal

Applied in this order.

1. **Test crates.** Unless `--include-tests`, remove test crates and their edges.
2. **Transitive reduction**, judged against all edges left by step 1. For an edge `A -> B`:
   - `B` internal, different namespace from `A`: keep.
   - `B` internal, same namespace: remove if `A` has another direct dependency `C` in that
     namespace from which `B` is reachable using only edges within that namespace.
   - `B` external: remove if another direct dependency `C` of `A`, in `A`'s namespace, also
     declares `B` directly with the same feature set as `A` does.
3. **Implied dependencies.** Remove `A -> B` if `B` is listed under `implied_deps[P]` and
   `A -> P` is an edge left by step 2.

A crate's feature set for a dependency is the union of `features` across all its
dependency tables.

### 9.3 External nodes

For each remaining edge `A -> E` to an external package, the target is a node keyed by
(namespace of `A`, `E`, added features), placed in that namespace's subgraph. Added
features are `A`'s feature set for `E` minus the features in `[workspace.dependencies]`,
sorted. Label: `E`, or `E [f1, f2]` when there are added features.

### 9.4 Proxies

**Link crates.** If namespace `N` lists `L` in `link_crates`, an edge `A -> L` with `A` in
`N` and `L` outside `N` becomes `A -.-> proxy(N, L)`, plus a single `proxy(N, L) -.-> L`.

**Embed crates.** If namespace `H` lists `X` (a crate of another namespace) in
`embed_crates`, `H` hosts `X`. Each edge from `X` takes the first rule that applies:

1. `X -> T` with `T` in a host `H`: `proxy(H, X) -.-> T`.
2. `X -> L` where a host `H` lists `L` in `link_crates` and `L` is outside `H`:
   `proxy(H, X) -.-> proxy(H, L)`, plus the single `proxy(H, L) -.-> L`. With several
   such hosts, the first by name.
3. Otherwise the edge is treated like any other crate's, including the link crate rule
   for `X`'s own namespace.

Each `proxy(H, X)` used by rule 1 or 2 gets a single `X -.-> proxy(H, X)`.

A proxy node exists only if at least one edge uses it. `proxy(N, C)` is drawn in the
subgraph of `N`.

### 9.5 Mermaid output

The output is byte-identical across implementations for the same input.

```
flowchart TB
  subgraph ns_<namespace> ["<namespace>"]
    <id>["<label>"]
  end
  <id> --> <id>
  <id> -.-> <id>
  style <id> fill:<hex>,stroke:<hex>
```

- Two-space indentation, LF line endings, one trailing newline.
- Order: subgraphs by namespace name. Inside a subgraph: crate nodes by name, then proxies
  by crate name, then external nodes by package then features. Then all edges by source id
  then target id. Then all `style` lines, subgraphs first, then nodes, each by id.
- Edges are `-.->` when they cross a namespace or touch a proxy, `-->` otherwise.
- Ids: `c_<crate>` for crates, `p_<ns>__<crate>` for proxies, `x_<ns>__<package>` for
  external nodes with `__<f1>_<f2>` appended when there are added features. Every character
  outside `[A-Za-z0-9_]` is replaced by `_`. Two nodes with the same id is a failure.

### 9.6 Colours

Colours are table lookups; no floating-point arithmetic.

- Namespace `i` (position in the sorted list of namespaces that have a node) uses entry
  `i mod 12` of the namespace palette for its subgraph. Its untagged crates use that
  stroke and fill `#ffffff`.
- A crate's tag is the first part of its tail when the tail has two or more parts and
  either that part is an approved prefix or the last part is `types`, `test` or `tests`.
  A tagged crate uses its namespace stroke and tag fill `h mod 8`, where `h` starts at 0
  and for each UTF-8 byte `b` of the tag becomes `(h * 31 + b) mod 2^32`.
- A proxy uses the colours of the crate's own namespace plus `stroke-dasharray:5 5`.
- External nodes use stroke `#606060`, fill `#d0d0d0`.

Namespace palette (stroke, fill):

| i | stroke | fill | i | stroke | fill |
|---|---|---|---|---|---|
| 0 | `#1f77b4` | `#dbe9f6` | 6 | `#00838f` | `#d3eef0` |
| 1 | `#d95f02` | `#fde3cf` | 7 | `#7f7f00` | `#eeeecc` |
| 2 | `#2ca02c` | `#dcf0dc` | 8 | `#37474f` | `#dde3e6` |
| 3 | `#9467bd` | `#e9e0f3` | 9 | `#b8860b` | `#f6ecc9` |
| 4 | `#8c564b` | `#eadfdc` | 10 | `#283593` | `#d9dcf2` |
| 5 | `#c2185b` | `#f8d7e3` | 11 | `#e53935` | `#fbd9d8` |

Tag fills, 0 to 7: `#8dd3c7`, `#ffffb3`, `#bebada`, `#fb8072`, `#80b1d3`, `#fdb462`,
`#b3de69`, `#fccde5`.

## 10. Failures

Each of these prints a message to stderr and exits 1. Nothing is skipped silently and
nothing panics.

- No `Cargo.toml` found.
- `cargo metadata` fails.
- No config, or config unreadable, unparseable, of the wrong shape, or with unknown keys.
- Any workspace or crate `Cargo.toml` unreadable or unparseable.
- A file cannot be written (`--fix`, `--graph`, `init`).
- `init` when `stow.toml` exists.
- Two graph nodes with the same id.

## 11. Tests

Tests are language-neutral so one suite runs against both implementations. They live in
`tests/cases`, one `.case` file per run; `tests/cases/README.md` defines the format and
what a runner must do.

- A case holds a small workspace, the command line, and the expected exit code, standard
  output and resulting files (manifests after `--fix`, graph text).
- Any file a run creates or changes must be expected, so an untouched file is asserted
  simply by not listing it.
- Fixtures need no network: stow never resolves dependencies, so a dependency can name
  any package.
- Every check has passing and failing cases, in plain and `target.<cfg>` tables where it
  applies.
- `--fix` cases cover comment movement, free-standing comments, kept keys, unconvertible
  entries, untouched files and idempotence.
- Graph cases cover each reduction rule, implied dependencies, feature-split external
  nodes, link proxies, embed proxies and their combination.

## 12. Out of scope

- Manifest formatting.
- SVG output and any graphviz dependency.
- The legacy flat config.
- A richer rule language.
