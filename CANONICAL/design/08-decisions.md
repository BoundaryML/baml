# Decision register

**Status:** Canonical. This register adopts the settled August 10 D1–D16 overlay and reconciles it with code and the earlier Studio/profiler designs.

## Precedence

1. This directory is current design authority.
2. Live code is authority for what is built.
3. The settled D1–D16 query decisions control target query semantics.
4. The August 6 profiler/Studio designs supply non-conflicting product, capture, ingest, UI, reliability and storage detail.
5. Older plans and research are historical evidence.

## Settled architecture

| ID | Decision | Consequence |
|---|---|---|
| D1 | Population CCT aggregates plus explicitly retained exact instances | No default row per call; retained counts are lower bounds on traffic |
| D2 | Occurrence, unique-root, and scalar/path value grains all remain available | One deduplicated value model serves all three; default UI grain deferred |
| D3 | DataFusion/BAML coordinates public SQL; ClickHouse remains the hosted non-value engine | Same logical semantics, different physical providers |
| D4 | Backend-only functions fail explicitly where unavailable | Local planning returns E_BACKEND_CAPABILITY; never silently uploads |
| D5 | ClickHouse extensions are allowlisted inside the BAML/DataFusion grammar | No raw ClickHouse parser/passthrough |
| D6 | A backend-neutral baml_query crate owns catalog, planning, scope, budgets and provider contracts | SQLite/Parquet/direct-artifact are implementation choices |
| D7 | Virtual BAML values use ordinary SQL equality, comparison, and subscript syntax | DataFusion lowers them to internal hydration/path/type expressions; helper chains are not the primary public contract |
| D8 | ClickHouse stores no customer value content | Only non-content opaque handles plus occurrence/availability metadata may be resident |
| D9 | One streaming SQL path handles large finite value queries | Size alone does not require a background scan; budget/cancel remain |
| D10 | Ordinary SQL binds a fixed snapshot | Catalog, generation, watermark, scope and provider handles stay stable |
| D11 | Accepted hosted S3 evidence is indefinite by default | Explicit authorized erasure is the deletion path; optional retention windows deferred |
| D12 | Data-level value failures preserve typed unknowns and make results incomplete | Missing/redacted/corrupt is not NULL or non-match |
| D13 | Every SQL stream ends with a typed out-of-band outcome | No terminal outcome means no complete success claim |
| D14 | V1 exposes only platform-owned functions | No user UDFs, CREATE FUNCTION, plugins or arbitrary query code |
| D15 | Durable running state is visible to ordinary SQL | Pending fields and so-far counters are distinct from query completeness |
| D16 | Preserve versioned logical relation names | Physical mappings may change behind the public catalog |

## Non-query decisions carried forward

| Topic | Decision |
|---|---|
| Product surface | Initiative name Project Studio; target user commands are baml playground and baml query |
| UI query boundary | Private fold-engine RPC for live/debug UI; public SQL for portable questions |
| Evidence authority | Local .baml / hosted accepted S3 artifacts and CAS |
| Hosted control authority | PostgreSQL |
| Queue | SQS Standard carries replaceable at-least-once pointers |
| Hosted analytical authority | ClickHouse is rebuildable and non-value |
| Capture/upload separation | Instrumentation, drain adapter, durable spool and transport are separate responsibilities |
| Cross-process execution | Related runs with explicit links, not one merged exact graph |
| Snapshot identity | Content identity; deployment/release/git/build are dimensions |
| Cross-revision function identity | Revision ID names the exact compiled program; definition key groups the logical function; local definition hash covers only that function's own compiled signature and bytecode |
| Active execution | A bounded rebuildable active index may accelerate discovery but is never evidence |
| Duplicate delivery | Deterministic logical IDs/batches; serving semantics must be duplicate/conflict safe |
| Structural exhaustion target | fail_run recommended default; abort_process strict opt-in; continue_incomplete only for diagnostic admission |
| Local control DB | Keep one control.sqlite for non-rebuildable sync/policy/operation state |
| Initial hosted topology | Single cell per v1 project |
| Hosted infrastructure | Terraform, ECS/Fargate, S3, SQS, PostgreSQL, ClickHouse Cloud, static SPA, OIDC |

The structural-exhaustion policy is implemented: **BAML_PROFILE_EXHAUSTION** selects fail_run (default) / abort_process (strict opt-in) / continue_incomplete, with every shed persisted as declared loss evidence. Exact per-environment defaults remain X1 policy work.

## Explicit supersessions

| Older statement | Canonical replacement |
|---|---|
| Public ClickHouse dialect everywhere | Public BAML/DataFusion SQL; allowlisted CH extensions |
| Local chDB/clickhouse-local | Backend-neutral local providers selected per relation |
| DataFusion rejected | DataFusion owns planning/coordination locally and hosted |
| Same ClickHouse DDL local and cloud | Same logical catalog; trusted physical mappings differ |
| Persistent value_scalars/value_nodes/previews in CH | No customer value-derived content in CH |
| Public `value_at`/`value_field`/typed-conversion helper chains for ordinary traversal and comparison | Ordinary SQL operators and subscripts over the virtual BAML value type; internal lowering names remain private (D7) |
| Optional CH chunk-KV hydration cache | Not allowed by D8 |
| Hosted deterministic per-tenant CID token fixed | Handle representation and identity-based optimization are deferred by X4; public semantic equality is fixed by D7 |
| Evidence ledgers/freshness footer are enough | Keep ledgers, plus mandatory query_outcome |
| Large value candidate sets require deferred scans | Ordinary queries stream; durable background mode is separate |
| Hosted automatic age/size retention | Accepted evidence indefinite; explicit erasure only |
| A row/observation per every call | Population CCT plus retained exact instances |
| BQL or StudioQueryV1 as future public surface | Superseded, not deferred |
| BQL already deleted | Still built; delete only after SQL parity |
| baml studio already removed | Still built; command consolidation is implementation work |
| Boundary IDs are ULIDs | Current BoundaryId payload is UUIDv4; chronology comes from created_ms/history prefix |
| CAS packs named .bpk1/.bpki with zstd | Current files are .bamlpack/.bamlpack.idx and v1 writes raw records |
| BCCT header is 32 bytes | Current header is 112 bytes |
| Positive and negative zero canonicalize together | Current canonical codec keeps them distinct |
| index.jsonl is current run discovery | No implementation; current reader scans boundary meta files |
| Server shedding ladder is shipped | The fail_run/abort_process/continue_incomplete policy is implemented (C1); the multi-step server ladder beyond it remains hosted work |
| Full-trace writer is shipped | Not implemented; explicitly absent for v1 (deferred) |
| Continuous value drain/promotion path is fully shipped | The CLI drains continuously off-thread (C1); production helper staging/promotion wiring remains deferred |

## Rejected v1 alternatives

- Traffic-proportional all-call fact table.
- Raw physical database access.
- A second query grammar.
- Silent local-to-hosted routing.
- LLMs or arbitrary user code in query execution.
- ClickHouse customer value bodies or derived search indexes.
- Required public helper chains for ordinary BAML value equality, traversal,
  or scalar comparison.
- Loose prototype value storage or a second CID space.
- NULL as the only evidence/availability model.
- Per-batch budgets presented as query-global.
- Queue ordering or merge timing as correctness.

## Internal inconsistency resolved

The latest decision overlay contains one older sentence saying a large candidate set requires a checkpointed deferred scan. Its later D9 decision explicitly says candidate size alone does not require deferred execution and specifies one streaming path. D9 controls. Durable background operations remain deferred only for survival/progress/persisted-result product semantics.

## Change discipline

A decision change must:

- state which existing ID or carried-forward decision it replaces;
- update the component and storage documents;
- describe migration/compatibility impact on catalog and saved queries;
- update deferred and delivery registers; and
- identify whether the change is semantic, policy, benchmark-owned, or implementation-only.
