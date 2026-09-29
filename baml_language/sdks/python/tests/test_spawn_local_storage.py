"""
Tests for seeding `baml.SpawnLocalStorage` from a host call: values passed as
`spawn_local_storage` must be readable in the called function and in every
thread it spawns.
"""

import pytest

from baml_bridge import BamlRuntime, call_function, call_function_sync


SPAWN_LOCAL_STORAGE_BAML = """\
function RequestId() -> baml.SpawnLocalStorage<string> {
    baml.SpawnLocalStorage.new("request_id", "none")
}

function Tags() -> baml.SpawnLocalStorage<map<string, string>> {
    let empty: map<string, string> = {};
    baml.SpawnLocalStorage.new("tags", empty)
}

function ReadRequestId() -> string {
    RequestId().get()
}

function ReadInNestedSpawns() -> string {
    let outer = spawn {
        let inner = spawn { RequestId().get() };
        await inner
    };
    await outer
}

function ReadAfterRunInSpawn() -> string {
    let child = RequestId().run("overridden", () -> baml.future.Future<string, never> {
        spawn { RequestId().get() }
    });
    `${await child}|${RequestId().get()}`
}

function ReadTag(key: string) -> string {
    let child = spawn { Tags().get().get(key) ?? "missing" };
    await child
}
"""


def make_runtime() -> BamlRuntime:
    return BamlRuntime.initialize_runtime(".", {"main.baml": SPAWN_LOCAL_STORAGE_BAML})


class TestSpawnLocalStorage:
    def test_default_without_host_values(self):
        rt = make_runtime()
        result = call_function_sync(rt, "ReadRequestId", {})
        assert result.result() == "none"

    def test_host_value_visible_in_call(self):
        rt = make_runtime()
        result = call_function_sync(
            rt, "ReadRequestId", {}, spawn_local_storage={"request_id": "req-1"}
        )
        assert result.result() == "req-1"

    def test_host_value_inherited_by_nested_spawns(self):
        rt = make_runtime()
        result = call_function_sync(
            rt, "ReadInNestedSpawns", {}, spawn_local_storage={"request_id": "req-2"}
        )
        assert result.result() == "req-2"

    def test_run_inside_call_overrides_host_value(self):
        rt = make_runtime()
        result = call_function_sync(
            rt, "ReadAfterRunInSpawn", {}, spawn_local_storage={"request_id": "req-3"}
        )
        assert result.result() == "overridden|req-3"

    def test_structured_host_value(self):
        rt = make_runtime()
        result = call_function_sync(
            rt,
            "ReadTag",
            {"key": "user"},
            spawn_local_storage={"tags": {"user": "alice"}},
        )
        assert result.result() == "alice"

    def test_mismatched_host_value_reads_default(self):
        rt = make_runtime()
        result = call_function_sync(
            rt, "ReadRequestId", {}, spawn_local_storage={"request_id": 42}
        )
        assert result.result() == "none"

    def test_values_do_not_leak_between_calls(self):
        rt = make_runtime()
        call_function_sync(
            rt, "ReadRequestId", {}, spawn_local_storage={"request_id": "req-4"}
        )
        result = call_function_sync(rt, "ReadRequestId", {})
        assert result.result() == "none"

    @pytest.mark.asyncio
    async def test_async_call(self):
        rt = make_runtime()
        result = await call_function(
            rt, "ReadInNestedSpawns", {}, spawn_local_storage={"request_id": "req-5"}
        )
        assert result.result() == "req-5"
