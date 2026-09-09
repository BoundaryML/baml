"""Run isolated Rust API probes without a BAML build or engine.

All negative probes must fail with the expected Rust diagnostic code.
Binaries are built in a temporary directory and removed after execution.
"""
from pathlib import Path
import json
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
NEGATIVE = {
    "derive_clone_fails.rs": "E0599",
    "sized_input_fails.rs": "E0038",
    "borrowed_options_fail.rs": "E0597",
    "generic_dyn_fails.rs": "E0038",
    "container_coercion_fails.rs": "E0308",
}
POSITIVE = ["positive.rs", "automatic_input.rs"]

print(subprocess.check_output(["rustc", "--version"], text=True).strip())
with tempfile.TemporaryDirectory(prefix="baml-interface-rust-probes-") as tmp:
    for source in POSITIVE:
        binary = Path(tmp) / source.removesuffix(".rs")
        subprocess.run(
            ["rustc", "--edition=2021", str(ROOT / source), "-o", str(binary)],
            check=True,
        )
        subprocess.run([str(binary)], check=True)
    for source, expected in NEGATIVE.items():
        result = subprocess.run(
            ["rustc", "--edition=2021", "--error-format=json", str(ROOT / source), "-o", str(binary)],
            capture_output=True,
            text=True,
        )
        diagnostics = [json.loads(line) for line in result.stderr.splitlines() if line.startswith("{")]
        codes = {(item.get("code") or {}).get("code") for item in diagnostics}
        assert result.returncode != 0 and expected in codes, (source, result.stderr)
        print(f"PASS: {source} rejected with {expected}")
print("These are host-language/model probes, not an implemented BAML interface bridge test.")
