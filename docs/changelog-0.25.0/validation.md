# BAML 0.25.0 validation

The fixed lower/upper sources are recorded in [the inventory](step1a-release-inventory.md). Runtime/code-generation validation uses macOS ARM64 artifacts from [release run 42150548387](https://github.com/BoundaryML/baml/actions/runs/42150548387) at `a2f6d5403f399b0b3cc1a0902b0b3dd208c6043a`. They were downloaded while the complete publishing run was still in progress. Built-product validation does not establish successful canary promotion.

| Artifact | SHA-256 |
| --- | --- |
| `baml-language-0.25.0-aarch64-apple-darwin.tar.gz` | `0a802d2acb760ba9f8274f7ccc084d612efa3a1b13ca90de877c4a653aa7a28a` |
| `baml_bridge-0.25.0-cp310-abi3-macosx_11_0_arm64.whl` | `000153d43d4dca1128f8075276ff12d1787d861ba42bc6344df81912169ae3c1` |
| `libbaml_cffi-aarch64-apple-darwin.dylib` | `af801d1d99ac5ba98c2e08cc32d0b6d89496d95793dce0a20c11e141ba3047df` |

## Results

| Check | Evidence and limit |
| --- | --- |
| All package-name configurations | Release-build compiler generates the four configurations shown in the post. Verified Python directory/import path, Go package appapi and Identity signature, Rust Cargo package/lib app_api and identity(String), and TypeScript generated-api/Identity. Go/Rust/TS host execution was not repeated. |
| Python nested package | Matching 0.25.0 wheel and CFFI library execute sync/async identity and nominal Person round trips. Root trace/experimental shortcuts resolve. No parent __init__.py is created. |
| Python relocation | Copied the generated package under another name and repeated all runtime checks in a fresh interpreter. Separate interpreters intentionally avoid the still-shared runtime limitation. |
| Older CLI | Published 0.24.0 compiler ignores package_name and generates baml_sdk. The post requires the 0.25.0 CLI. |
| Ownership guard | A named Go output containing handwritten keep.txt refuses generation before earlier Python output is created. The handwritten file remains unchanged. |
| Stdlib warnings | Identical one-function project: 0.24.0 check with display_all_warnings reports ten stdlib E0146 warnings; release-build 0.25.0 reports none. Both checks complete successfully. |
| Finalizer docs | Target describe output contains the corrected catch_all arm. The pattern executes with a missing temporary path and handles its error. No syntax feature is claimed. |
| Query example | The exact SQL parses and executes against a local format-5 recording. This validates the query, not production of redacted recordings. |

Source inspection confirms BLOB_VERSION remains 5 and READABLE_BLOB_VERSIONS gains 6. `cargo test -p btel_reader --lib redacted_values_are_withheld_present_values` passes, exercising redacted state, rendering, and navigation. `cargo test -p btel_snapshot --test format frozen_v6_redactions_decode_with_verified_content_ids` also passes against the frozen v6 bytes and content IDs. A separate debug compiler built from the same pinned source produces the same package layout; the runtime checks above use release-build artifacts. No live provider or model request was made. No runtime performance claim is inferred from article-chart changes.

The production website build passes and generates both release routes. The actual release loader includes 0.25.0 above 0.24.0 and excludes both contributor checklists. Inventory/aggregation/quote checks, applicable pre-commit hooks, and git diff --check pass.
