"""End-to-end check of pipeline-style codegen.

Drives codegen from real `.baml` source through the full
`baml_project::build_symbol_pool` path (parse → HIR → TIR → SymbolPool
→ emitter). Everything lives at the root user namespace and the
fixture simulates a call graph using `baml.http.fetch` instead of LLM
calls:

- `FetchOrder(id) -> Pending | Completed | Cancelled` — leaf stage,
  one `baml.http.fetch` of httpbin/delay/1 then a union arm.
- `ChargePayment(amount) -> Charged | Declined` — leaf stage, same
  fetch then a union arm.
- `ProcessPending(p) -> string` — orchestration, one fetch.
- `ProcessCompleted(c) -> string` — orchestration, fetch + nested call
  to `ChargePayment` with a `match` over its union.
- `HandleOrder(id) -> string` — top of the call graph, fetches an
  order and `match`es each variant to one of the process functions.

Plus `testset` / `test` blocks in `baml_src/tests.baml` covering the
nested-testset and dynamic-test-generation shapes from
`baml_tests/projects/compiles/testset_nested` and
`testset_dynamic` / `testset_vibes_nested`. The testsets are exercised
at codegen time (build.rs fails on any compile diagnostic); pytest
asserts on the resulting SDK surface only.
"""


def test_root_imports_cleanly():
    import baml_sdk  # noqa: F401


# ---------------------------------------------------------------------------
# Union variant classes
# ---------------------------------------------------------------------------


def test_fetch_order_union_arms_are_emitted():
    import pydantic
    from baml_sdk import Pending, Completed, Cancelled

    for cls in (Pending, Completed, Cancelled):
        assert issubclass(cls, pydantic.BaseModel)

    assert set(Pending.model_fields) == {"id"}
    assert set(Completed.model_fields) == {"id", "total"}
    assert set(Cancelled.model_fields) == {"id", "reason"}


def test_charge_payment_union_arms_are_emitted():
    import pydantic
    from baml_sdk import Charged, Declined

    for cls in (Charged, Declined):
        assert issubclass(cls, pydantic.BaseModel)

    assert set(Charged.model_fields) == {"amount"}
    assert set(Declined.model_fields) == {"reason"}


# ---------------------------------------------------------------------------
# Leaf stages — each returns a union of the classes above.
# ---------------------------------------------------------------------------


def test_fetch_order_factory_bindings():
    import baml_sdk

    assert callable(baml_sdk.FetchOrder)
    assert callable(baml_sdk.FetchOrder_async)


def test_charge_payment_factory_bindings():
    import baml_sdk

    assert callable(baml_sdk.ChargePayment)
    assert callable(baml_sdk.ChargePayment_async)


# ---------------------------------------------------------------------------
# Orchestration layer — non-LLM bodies that `match` on a leaf's union
# and call the next stage. These exercise the codegen path for
# `let x = F(...); match (x) { ... }` lowering.
# ---------------------------------------------------------------------------


def test_process_pending_factory_bindings():
    import baml_sdk

    assert callable(baml_sdk.ProcessPending)
    assert callable(baml_sdk.ProcessPending_async)


def test_process_completed_factory_bindings():
    import baml_sdk

    assert callable(baml_sdk.ProcessCompleted)
    assert callable(baml_sdk.ProcessCompleted_async)


def test_handle_order_factory_bindings():
    # Top of the simulated call graph. Sync + async factories both
    # land at the root namespace alongside the leaf stages.
    import baml_sdk

    assert callable(baml_sdk.HandleOrder)
    assert callable(baml_sdk.HandleOrder_async)


# ---------------------------------------------------------------------------
# Inlined BAML files — verifies every source file is preserved verbatim
# for the runtime bootstrap, including the testset-bearing one.
# ---------------------------------------------------------------------------


def test_inlinedbaml_files_present():
    from pathlib import Path
    from baml_sdk.baml import _inlinedbaml

    assert "FILES" in dir(_inlinedbaml)
    actual_paths = {Path(p) for p in _inlinedbaml.FILES.keys()}
    expected_paths = {
        Path("types.baml"),
        Path("stages.baml"),
        Path("pipeline.baml"),
        Path("tests.baml"),
    }
    assert actual_paths == expected_paths
