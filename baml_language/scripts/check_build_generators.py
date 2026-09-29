#!/usr/bin/env python3
"""Compare standalone host generators with Cargo's production build outputs."""

import json
import os
from pathlib import Path
import subprocess
import tempfile

WORKSPACE = Path(__file__).resolve().parent.parent
OUTPUTS = {
    "bex_vm": {"nativefunctions_generated.rs", "aifunctions_generated.rs", "reflectfunctions_generated.rs"},
    "bex_vm_types": {"sys_op_generated.rs", "errors_generated.rs", "panics_generated.rs"},
    "sys_ops": {"io_generated.rs", "io_adapter.rs"},
    "sys_types": {"io_generated.rs", "runtime_io.rs"},
    "bex_project": {"stdlib_prefix.borsh"},
}
TOOLS = {"baml-generate-builtins", "baml-generate-stdlib"}


def cargo_build(*args):
    result = subprocess.run(
        ["cargo", "build", "--locked", "--message-format=json", *args],
        cwd=WORKSPACE, stdout=subprocess.PIPE, text=True, check=True,
    )
    return [json.loads(line) for line in result.stdout.splitlines() if line.strip()]


def main():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=WORKSPACE, text=True,
    ))
    packages = {package["id"]: package["name"] for package in metadata["packages"]}
    outputs = {}
    for message in cargo_build("-p", "bex_project", "--lib"):
        if message["reason"] == "compiler-artifact":
            assert message["target"]["name"] not in TOOLS, "A normal runtime build built a generator command"
        if message["reason"] == "build-script-executed":
            name = packages.get(message["package_id"])
            if name in OUTPUTS:
                outputs[name] = Path(message["out_dir"])
    assert outputs.keys() == OUTPUTS.keys(), f"Missing Cargo output directories: {OUTPUTS.keys() - outputs.keys()}"

    executables = {}
    for message in cargo_build(
        "-p", "baml_builtins2_codegen", "--bin", "baml-generate-builtins",
        "-p", "baml_db", "--bin", "baml-generate-stdlib",
    ):
        if message["reason"] == "compiler-artifact" and message.get("executable"):
            executables[message["target"]["name"]] = Path(message["executable"]).resolve()
    assert executables.keys() == TOOLS, f"Missing host executables: {TOOLS - executables.keys()}"

    # Once compiled, generators need neither a checkout as their working
    # directory nor Cargo, Git, protoc, or any other executable on PATH.
    env = {key: value for key, value in os.environ.items() if not key.startswith("CARGO_")}
    for key in ("CARGO", "OUT_DIR", "BAML_GIT_SHA", "RUSTC", "RUSTC_WRAPPER", "PROTOC"):
        env.pop(key, None)
    env["PATH"] = ""
    with tempfile.TemporaryDirectory(prefix="baml build generators ") as directory:
        root = Path(directory)
        for tool in TOOLS:
            command = [str(executables[tool])]
            result = subprocess.run(command + ["--help"], cwd=root, env=env, capture_output=True, text=True)
            assert result.returncode == 0 and "Usage:" in result.stdout, result
            invalid = [[], ["--out-dir"], ["--unknown"]]
            if tool == "baml-generate-builtins":
                invalid.append(["unknown-crate", "--out-dir", str(root / "invalid")])
            for args in invalid:
                result = subprocess.run(command + args, cwd=root, env=env, capture_output=True, text=True)
                assert result.returncode != 0 and "Usage:" in result.stderr, result
        assert not list(root.iterdir()), "Argument validation wrote files"

        for name, filenames in OUTPUTS.items():
            destination = root / name
            if name == "bex_project":
                command = [str(executables["baml-generate-stdlib"])]
            else:
                command = [str(executables["baml-generate-builtins"]), name]
            command += ["--out-dir", str(destination)]
            subprocess.run(command, cwd=root, env=env, check=True)
            assert {path.name for path in destination.iterdir()} == filenames, name
            for filename in filenames:
                actual = (destination / filename).read_bytes()
                expected = (outputs[name] / filename).read_bytes()
                assert actual == expected, f"{name}/{filename} differs from Cargo's output"
            print(f"ok: {name} outputs match Cargo without Cargo on PATH", flush=True)
        assert {path.name for path in root.iterdir()} == OUTPUTS.keys(), "Unexpected generation outputs"
    print("ok: all 11 outputs match; normal builds do not build the standalone commands", flush=True)


if __name__ == "__main__":
    main()
