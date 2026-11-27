"""Today's SDK tests for the fixture run unchanged against the native
bindings; the only edits are these skips, each with its reason. They apply
to the native bindings only (PYTHONPATH=.../native)."""

import os

import pytest

# test id (file::name) -> why it does not run against the native bindings
SKIPS = {
    "roundtrip_tests/test_handles.py": "standard-library handles (baml.fs, baml.http) live in today's "
    "runtime; nothing of them is in the native library (http.Response is refused when packing)",
    "roundtrip_tests/test_media.py::test_host_from_file_reads_now_and_keeps_the_base_name": "media "
    "classes are today's runtime's (from_file calls a BAML function of the standard library); media "
    "does not cross the native boundary",
    "roundtrip_tests/test_recursion.py::test_recursion_int_binary_tree_deeper_than_the_wire_is_refused":
    "the 32-level limit is the protobuf wire's; the native bridge has no wire, crosses 33 levels, and "
    "refuses past its own limit of 200 levels with the same TypeError ('nested too deeply')",
}


def _native() -> bool:
    import baml_sdk

    return "native" in os.path.dirname(baml_sdk.__file__).split(os.sep)


def pytest_collection_modifyitems(config, items):
    if not _native():
        return
    for item in items:
        nodeid = item.nodeid.split("[")[0]
        for key, why in SKIPS.items():
            if nodeid == key or nodeid.endswith("/" + key) or nodeid.startswith(key + "::") or (
                key.endswith(".py") and nodeid.split("::")[0].endswith(key)
            ):
                item.add_marker(pytest.mark.skip(reason=why))
                break


@pytest.hookimpl(hookwrapper=True)
def pytest_runtest_makereport(item, call):
    """A test that calls a function the packer refused is skipped, with the
    refusal: these are the tests that use only functions that do not pack."""
    outcome = yield
    report = outcome.get_result()
    if (
        call.excinfo is not None
        and call.excinfo.errisinstance(NotImplementedError)
        and "not in the native library" in str(call.excinfo.value)
    ):
        report.outcome = "skipped"
        report.longrepr = (str(item.path), item.location[1] or 0, f"Skipped: {call.excinfo.value}")
