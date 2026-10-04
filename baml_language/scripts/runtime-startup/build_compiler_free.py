#!/usr/bin/env python3
"""Build diagnostic hosts without either route to the runtime compiler.

The native bridge currently imports runtime traits/types from bex_project,
which also depends on the compiler. Generate a runtime-only facade from its
actual runtime exports and Bex implementation, rebuild the unchanged bridge
and native provider against it, then link the matched control/profile hosts.
No shipping source or artifact format is changed. All generated files stay in
the requested output directory. Uses existing release Cargo fingerprints.
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--full-hosts", type=Path, required=True)
    parser.add_argument("--release-dir", type=Path, default=Path("target/release"))
    parser.add_argument("--jobs", type=int, default=2)
    args = parser.parse_args()
    if sys.platform != "darwin" or args.jobs < 1:
        parser.error("Requires macOS and a positive --jobs value.")
    root = Path(__file__).resolve().parents[2]
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    release = args.release_dir.resolve()
    full = args.full_hosts.resolve()
    original = json.loads((full / "build-metadata.json").read_text())
    fingerprints = release / ".fingerprint"
    cache = {}

    def artifact(name, fingerprint):
        key = (name, fingerprint)
        if key not in cache:
            candidates = list(fingerprints.glob(f"*/lib-{name}.json"))
            path = next(
                path
                for path in candidates
                if int.from_bytes(
                    bytes.fromhex(path.with_suffix("").read_text()), "little"
                )
                == fingerprint
            )
            cache[key] = (path, json.loads(path.read_text()))
        return cache[key]

    def dependencies(info, names=None):
        return {
            name: artifact(name, fingerprint)
            for _, name, _, fingerprint in info["deps"]
            if name != "build_script_build" and (names is None or name in names)
        }

    def library(path):
        package, fingerprint = path.parent.name.rsplit("-", 1)
        prefix = release / "deps" / f"lib{package.replace('-', '_')}-{fingerprint}"
        for suffix in (".rlib", ".dylib"):
            candidate = prefix.with_name(prefix.name + suffix)
            if candidate.exists():
                return candidate
        raise FileNotFoundError(prefix)

    def compile_library(name, source, selected, output, features, extra_env=None):
        command = [
            "rustc",
            "--edition=2024",
            "--crate-name",
            name,
            "--crate-type",
            "rlib",
            "-C",
            "opt-level=3",
            "-C",
            "codegen-units=1",
            "-C",
            "embed-bitcode=yes",
            "-C",
            "strip=symbols",
            "-C",
            "panic=unwind",
            "-C",
            f"metadata=compiler_free_{output.parent.name}_{name}",
            str(source),
            "-o",
            str(output),
            "-L",
            f"dependency={release}/deps",
            "-L",
            f"dependency={output.parent}",
        ]
        for feature in features:
            command += ["--cfg", f'feature="{feature}"']
        for dependency, path in selected.items():
            command += ["--extern", f"{dependency}={path}"]
        print("Building", output.parent.name, name, flush=True)
        subprocess.run(command, env=os.environ | (extra_env or {}), check=True)
        return command

    # Preserve the runtime types, error conversions, and exact Bex trait/impl.
    project_source = (root / "crates/bex_project/src/lib.rs").read_text()
    facade = project_source[: project_source.index("mod bex;")]
    facade += f'#[path = "{root / "crates/bex_project/src/bex.rs"}"]\nmod bex;\n'
    facade += project_source[
        project_source.index("pub struct BexArgs") : project_source.index(
            "/// Compile a BAML project"
        )
    ]
    assert "baml_db" not in facade and "runtime_compiler()" not in facade
    (out / "runtime-facade.rs").write_text(facade)
    for variant in ("control", "profile"):
        source = (full / f"{variant}.rs").read_text()
        assert source.count("Some(bex_project::runtime_compiler())") == 2
        (out / f"{variant}.rs").write_text(
            source.replace("Some(bex_project::runtime_compiler())", "None")
        )

    proto_candidates = list(
        (release / "build").glob("bridge_ctypes-*/out/baml_bridge.cffi.v1.rs")
    )
    assert proto_candidates
    proto = max(proto_candidates, key=lambda path: path.stat().st_mtime)
    # The generated schema used by both builds must be identical.
    assert (
        len(
            {hashlib.sha256(path.read_bytes()).hexdigest() for path in proto_candidates}
        )
        == 1
    )
    metadata = {
        "without_compiler": True,
        "strategy": "runtime-only project facade",
        "commands": {},
    }
    tasks = []
    for revision in ("baseline", "candidate"):
        directory = out / revision
        directory.mkdir(exist_ok=True)
        paths = {
            name: Path(path) for name, path in original[revision]["libraries"].items()
        }
        infos = {name: json.loads(path.read_text()) for name, path in paths.items()}
        facade_names = (
            "bex_engine",
            "bex_external_types",
            "bex_heap",
            "bex_vm_types",
            "sys_ops",
            "sys_types",
            "indexmap",
            "thiserror",
            "async_trait",
        )
        project_dependencies = dependencies(infos["bex_project"], facade_names)
        selected = {
            name: library(project_dependencies[name][0]) for name in facade_names
        }
        facade_library = directory / "libbex_project.rlib"
        commands = [
            compile_library(
                "bex_project", out / "runtime-facade.rs", selected, facade_library, []
            )
        ]
        native_dependencies = dependencies(infos["sys_native"])
        bridge_path, bridge_info = native_dependencies["bridge_ctypes"]
        bridge_dependencies = dependencies(bridge_info)
        bridge_selected = {
            name: library(path)
            for name, (path, _) in bridge_dependencies.items()
            if name != "bex_project"
        }
        bridge_selected["bex_project"] = facade_library
        bridge_library = directory / "libbridge_ctypes.rlib"
        commands.append(
            compile_library(
                "bridge_ctypes",
                root / "crates/bridge_ctypes/src/lib.rs",
                bridge_selected,
                bridge_library,
                json.loads(bridge_info["features"]),
                {"OUT_DIR": str(proto.parent)},
            )
        )
        native_selected = {
            name: library(path)
            for name, (path, _) in native_dependencies.items()
            if name != "bridge_ctypes"
        }
        native_selected["bridge_ctypes"] = bridge_library
        native_library = directory / "libsys_native.rlib"
        commands.append(
            compile_library(
                "sys_native",
                root / "crates/sys_native/src/lib.rs",
                native_selected,
                native_library,
                json.loads(infos["sys_native"]["features"]),
                {"BAML_HOST_TARGET": "aarch64-apple-darwin"},
            )
        )
        metadata[revision] = {
            "host": original[revision]["host"],
            "libraries": {
                name: str(path) for name, path in paths.items() if name != "bex_project"
            },
            "rebuilt": {
                "bex_project": str(facade_library),
                "bridge_ctypes": str(bridge_library),
                "sys_native": str(native_library),
            },
            "bridge_fingerprint": str(bridge_path),
        }
        metadata["commands"][revision] = commands
        for variant in ("control", "profile"):
            command = [
                "rustc",
                "--edition=2024",
                "-C",
                "opt-level=3",
                "-C",
                "lto=fat",
                "-C",
                "codegen-units=1",
                "-C",
                "strip=symbols",
                "-C",
                "panic=unwind",
                str(out / f"{variant}.rs"),
                "-o",
                str(out / f"{revision}-{variant}-host"),
                "-L",
                f"dependency={release}/deps",
                "-L",
                f"dependency={directory}",
            ]
            for name, path in paths.items():
                if name != "bex_project":
                    command += [
                        "--extern",
                        f"{name}={native_library if name == 'sys_native' else library(path)}",
                    ]
            tasks.append((f"{revision} {variant}", command))
    metadata["sources"] = {
        str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
        for pattern in (
            "crates/bex_project/src/lib.rs",
            "crates/bex_project/src/bex.rs",
            "crates/bridge_ctypes/src/*.rs",
            "crates/sys_native/src/*.rs",
        )
        for path in root.glob(pattern)
    }
    metadata["generated_proto_sha256"] = hashlib.sha256(proto.read_bytes()).hexdigest()
    metadata["host_commands"] = {label: command for label, command in tasks}
    (out / "build-metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")

    def build_host(task):
        label, command = task
        print("Linking", label, flush=True)
        subprocess.run(command, check=True)
        print("Finished", label, flush=True)

    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        for _ in pool.map(build_host, tasks):
            pass


if __name__ == "__main__":
    main()
