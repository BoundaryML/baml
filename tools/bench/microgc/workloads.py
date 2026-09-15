#!/usr/bin/env python3

from dataclasses import dataclass
from pathlib import Path
from typing import Dict


HEAP_STATS_FIELDS = (
    "total_objects",
    "compile_time_objects",
    "runtime_objects",
    "active_handles",
    "tlab_chunks",
    "reserved_slots",
    "gen0_slots",
    "gen1_slots",
    "gen2_slots",
    "gen0_capacity_slots",
    "gen1_capacity_slots",
    "gen2_capacity_slots",
    "scratch_capacity_slots",
    "allocations_since_gc",
    "object_slot_bytes",
    "tracked_slot_capacity_bytes",
    "permit_holder_slots",
    "allocator_blocks_in_use",
    "allocator_bytes_in_use",
    "allocator_bytes_high_water",
    "allocator_bytes_reserved",
)


@dataclass(frozen=True)
class Workload:
    name: str
    description: str
    batch_size: int
    body: str

    def source(self) -> str:
        return f"{format_heap_stats()}\n\n{self.body.rstrip()}\n\n{main_function(self.batch_size)}"


def format_heap_stats() -> str:
    lines = [
        "function format_heap_stats(sample: int, elapsed_ms: bigint, iterations: int, stats: baml.sys.HeapStats) -> string {",
        '    "sample="',
        "        + string.from(sample)",
        '        + " elapsed_ms="',
        "        + string.from(elapsed_ms)",
        '        + " iterations="',
        "        + string.from(iterations)",
    ]
    for field in HEAP_STATS_FIELDS:
        lines.extend((f'        + " {field}="', f"        + string.from(stats.{field})"))
    lines.append("}")
    return "\n".join(lines)


def main_function(batch_size: int) -> str:
    return f"""function main() -> null {{
    let started = baml.time.Instant.now();
    let sample = 0;
    let iterations = 0;

    while (true) {{
        let elapsed_ms = started.elapsed().to_milliseconds();
        let stats = baml.sys.heap_stats();
        baml.io.println(format_heap_stats(sample, elapsed_ms, iterations, stats));
        run_batch({batch_size});
        iterations += {batch_size};
        sample += 1;
    }};
}}
"""


def unused_scalars_body() -> str:
    return """function run_batch(count: int) -> null {
    let i = 0;
    while (i < count) {
        let a = 0.05;
        let b = 0.07;
        let c = 2.17;
        i += 1;
    }
}"""


def checksummed_scalars_body() -> str:
    return """function run_batch(count: int) -> null {
    let i = 0;
    let checksum = 0.0;
    while (i < count) {
        let a = 0.05;
        let b = 0.07;
        let c = 2.17;
        checksum += a + b + c;
        i += 1;
    }
    if (checksum <= 0.0) {
        baml.sys.panic("unexpected checksum");
    };
}"""


def float_array_body() -> str:
    return """function run_batch(count: int) -> null {
    let i = 0;
    while (i < count) {
        let values: float[] = baml.Array.filled(1000, 2.17);
        if (values.length() != 1000) {
            baml.sys.panic("unexpected array length");
        };
        i += 1;
    }
}"""


def float_map_body() -> str:
    set_lines = "\n".join(f'    let _ = values.set("k{index:04d}", 2.17);' for index in range(1000))
    return f"""function build_1000_float_map() -> map<string, float> {{
    let values: map<string, float> = {{}};
{set_lines}
    if (values.length() != 1000) {{
        baml.sys.panic("unexpected map length");
    }};
    values
}}

function run_batch(count: int) -> null {{
    let i = 0;
    while (i < count) {{
        let values = build_1000_float_map();
        if (values.length() != 1000) {{
            baml.sys.panic("unexpected returned map length");
        }};
        i += 1;
    }}
}}"""


def hardcoded_scalars_body() -> str:
    bindings = "\n".join(f"        let f{index:05d} = {(index + 1) / 10000:.4f};" for index in range(10000))
    return f"""function run_batch(count: int) -> null {{
    let i = 0;
    while (i < count) {{
{bindings}
        i += 1;
    }}
}}"""


WORKLOADS: Dict[str, Workload] = {
    workload.name: workload
    for workload in (
        Workload(
            "unused-scalars-3",
            "Three unused float scalar bindings per iteration",
            500_000,
            unused_scalars_body(),
        ),
        Workload(
            "checksummed-scalars-3",
            "Three float scalar bindings consumed by arithmetic per iteration",
            500_000,
            checksummed_scalars_body(),
        ),
        Workload(
            "float-array-1000",
            "A fresh 1,000-element float array per iteration",
            10_000,
            float_array_body(),
        ),
        Workload(
            "float-map-1000",
            "A fresh map populated by 1,000 hardcoded float insertions per iteration",
            100,
            float_map_body(),
        ),
        Workload(
            "hardcoded-scalars-10000",
            "Ten thousand hardcoded float locals without a selector, array, or map per iteration",
            10_000_000,
            hardcoded_scalars_body(),
        ),
    )
}


def write_workload(workload: Workload, output_root: Path) -> Path:
    app = output_root / workload.name
    source_dir = app / "baml_src"
    source_dir.mkdir(parents=True, exist_ok=True)
    (app / "baml.toml").write_text(f'[package]\nname = "microgc-{workload.name}"\n')
    (source_dir / "main.baml").write_text(workload.source())
    return app
