"""The generated SDK works under any package name and location.

The generator never names its own package, so `package_name = "pkg.sub"`
(nesting the SDK inside an application package) or a vendored copy under
another name behaves exactly like the default `baml_sdk`. Each case runs in
a fresh interpreter: one process holds one BAML program and type map, so
importing a second copy here would replace the `baml_sdk` that this suite's
other tests use.
"""

import importlib.util
import shutil
import subprocess
import sys
import textwrap
from pathlib import Path

# Imports the SDK named by argv[1] and exercises both type-map directions and
# the root-only `trace`/`experimental` shortcuts.
_EXERCISE = textwrap.dedent(
    """
    import importlib
    import sys

    from baml_bridge import get_type_map

    name = sys.argv[1]
    sdk = importlib.import_module(name)
    enums = importlib.import_module(name + ".enums")
    class_refs = importlib.import_module(name + ".class_refs")

    # Forward lookup: returned classes and enums are built through the type map.
    outer = class_refs.make_outer_sync(value=5)
    assert type(outer) is class_refs.Outer, type(outer)
    assert type(outer.inner) is class_refs.Inner, type(outer.inner)
    assert enums.pick_sentiment_sync(b=False) is enums.Sentiment.Negative

    # Reverse lookup: a generated class passed in resolves to its BAML name.
    value = enums.Enums(
        bare_enum=enums.Sentiment.Negative,
        variant_as_type=enums.Sentiment.Positive,
    )
    assert get_type_map().py_type_to_baml_type(enums.Enums) == "user.enums.Enums"
    echoed = enums.round_trip_enums_sync(e=value)
    assert type(echoed) is enums.Enums and echoed == value, echoed
    assert class_refs.round_trip_outer_sync(o=outer) == outer

    # Root-only shortcuts.
    assert sdk.trace is importlib.import_module(name + ".vendor.trace")
    assert sdk.experimental is importlib.import_module(name + ".experimental")
    assert callable(sdk.trace.instrument)

    assert "baml_sdk" not in sys.modules, "the relocated SDK imported baml_sdk"
    print("ok")
    """
)


def _generated_sdk() -> Path:
    spec = importlib.util.find_spec("baml_sdk")
    assert spec is not None and spec.submodule_search_locations
    return Path(next(iter(spec.submodule_search_locations)))


def _install(destination: Path) -> None:
    shutil.copytree(
        _generated_sdk(),
        destination,
        ignore=shutil.ignore_patterns("__pycache__"),
    )


def _exercise(import_root: Path, name: str) -> None:
    # `-c` puts the working directory first on sys.path, so `import_root` is
    # the only place the SDK can be found.
    result = subprocess.run(
        [sys.executable, "-c", _EXERCISE, name],
        cwd=import_root,
        capture_output=True,
        text=True,
        timeout=120,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert result.stdout.strip() == "ok"


# SDK_PARITY_LINT(skip): Python packages nest by import path; other SDKs name their package at generation
def test_sdk_works_nested_in_an_application_package_python_only(tmp_path):
    application = tmp_path / "pkg"
    application.mkdir()
    (application / "__init__.py").write_text("HAND_WRITTEN = True\n")
    _install(application / "sub")

    _exercise(tmp_path, "pkg.sub")

    assert (application / "__init__.py").read_text() == "HAND_WRITTEN = True\n"


# SDK_PARITY_LINT(skip): Python packages nest by import path; other SDKs name their package at generation
def test_sdk_works_when_copied_under_another_name_python_only(tmp_path):
    _install(tmp_path / "vendored_sdk")

    _exercise(tmp_path, "vendored_sdk")
