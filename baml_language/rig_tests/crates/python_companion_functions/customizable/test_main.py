"""Functions with attached companion methods from BEP-030.

Post-phase-6 layout (09b §3 / §4):
- Sync + async variants are sibling top-level names (`F` + `F_async`)
  at the leaf that routes the parent function, not split across
  `baml_sync` / `baml_async` subtrees.
- Companions are flat top-level bindings with a `__<companion>` suffix
  (e.g. `ExtractResume__build_request`), not attributes on the parent.
- The old `baml_sync/__init__.pyi` stub layout no longer exists.
"""


def test_sync_function_is_callable():
    import baml_sdk as b
    assert callable(b.ExtractResume)


def test_async_function_is_callable():
    import baml_sdk as b
    assert callable(b.ExtractResume_async)


def test_root_companions_attached():
    # Companions sit alongside the parent at the leaf, suffixed
    # with a double-underscore per 09b §3.
    import baml_sdk as b
    assert callable(b.ExtractResume__build_request)
    assert callable(b.ExtractResume__render_prompt)
    assert callable(b.ExtractResume__parse)


def test_root_companions_have_async_variants():
    import baml_sdk as b
    assert callable(b.ExtractResume__build_request_async)
    assert callable(b.ExtractResume__parse_async)


def test_namespaced_parent_and_companion():
    # Namespaced functions sit at `baml_sdk.<ns>.*`; companions follow
    # the same double-underscore convention.
    import baml_sdk as b
    assert callable(b.foo.ClassifySentiment)
    assert callable(b.foo.ClassifySentiment__build_request)


def test_parameter_names_reachable():
    # Factories expose their codegen-fixed parameter-name list via the
    # `param_names` attribute (set by `define_function`). The old shape
    # relied on `typing.get_type_hints(...)` returning annotations, but
    # factories are now plain callables with no signature annotations —
    # the authoritative parameter contract is `param_names`.
    import baml_sdk as b
    assert b.ExtractResume.param_names == ["resume"]
    assert b.ExtractResume__parse.param_names == ["json"]
