#!/usr/bin/env python3
"""Profile the production packed-host code path on macOS.

Build uses existing, fingerprint-matched release libraries and the real host
sources. It generates uninstrumented controls and instrumented hosts with the
unchanged release settings, leaving runtime sources untouched. Measure packs
identical fixtures with each matching compiler, interleaves controls/originals/
profiles, and preserves every parent/child timestamp. Finish builds before timing.
"""

import argparse
import hashlib
import json
import random
import statistics
import subprocess
import sys
from itertools import pairwise
from pathlib import Path


def _source_block(text):
    """Remove the extra indentation introduced by nesting these source literals."""
    first, *rest = text.split("\n")
    return "\n".join([first, *(line.removeprefix("    ") for line in rest)])


def build(args):
    root = Path(__file__).resolve().parents[2]
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    lib = (root / "crates/baml_pack_host/src/lib.rs").read_text()
    main = (root / "crates/baml_pack_host/src/main.rs").read_text()
    labels = [
        "constructor",
        "main_enter",
        "section_lookup",
        "decode",
        "argv",
        "sys_ops",
        "runtime_compiler",
        "recording_config",
        "engine_init",
        "function_info",
        "argv_parse",
        "json_source",
        "tokio_new",
        "dispatch",
        "record_exit",
        "shutdown",
        "telemetry_result",
        "spawn_error_drain",
        "before_tokio_drop",
        "after_tokio_drop",
        "before_engine_drop",
        "host_run_done",
    ]
    profile = _source_block("""
    mod profile {
        use std::sync::atomic::{AtomicU64, Ordering};
        #[repr(C)]
        struct Timespec { sec: i64, nsec: i64 }
        unsafe extern "C" { fn clock_gettime(clock: i32, time: *mut Timespec) -> i32; }
        static STAMPS: [AtomicU64; 22] = [const { AtomicU64::new(0) }; 22];
        const LABELS: [&str; 22] = LABELS_PLACEHOLDER;
        pub fn mark(id: usize) {
            let mut time = Timespec { sec: 0, nsec: 0 };
            // macOS CLOCK_MONOTONIC = 6, shared with the native parent helper.
            assert_eq!(unsafe { clock_gettime(6, &mut time) }, 0);
            STAMPS[id].store(time.sec as u64 * 1_000_000_000 + time.nsec as u64, Ordering::Relaxed);
        }
        pub struct RuntimeDrop;
        impl Drop for RuntimeDrop { fn drop(&mut self) { mark(19); } }
        extern "C" fn constructor() { mark(0); }
        #[used]
        #[unsafe(link_section = "__DATA,__mod_init_func")]
        static CONSTRUCTOR: extern "C" fn() = constructor;
        pub fn emit() {
            let values = STAMPS.iter().enumerate().filter_map(|(i,t)| {
                let n=t.load(Ordering::Relaxed);
                (n!=0).then(|| format!("\\\"{}\\\":{}", LABELS[i], n))
            }).collect::<Vec<_>>();
            eprintln!("BAML_STARTUP_PROFILE:{{{}}}",values.join(","));
        }
    }
    """).replace("LABELS_PLACEHOLDER", json.dumps(labels))

    def replace(text, old, new, expected=1):
        assert text.count(old) == expected, (old, text.count(old))
        return text.replace(old, new)

    instrumented = lib
    instrumented = replace(
        instrumented,
        "    let argv = build_argv(&target.subcommand_name);",
        _source_block("""    let argv = build_argv(&target.subcommand_name);
        crate::profile::mark(4);
        let sys_ops = Arc::new(sys_native::SysOps::native());
        crate::profile::mark(5);
        let runtime_compiler = Some(bex_project::runtime_compiler());
        crate::profile::mark(6);
        let recording = bex_engine::TelemetryRecording::from_boundary_env()
            .unwrap_or_else(|| bex_engine::TelemetryRecording::user_files(
                btel_settings::publisher::RecordingConfig::default(),
            )).with_host("pack");
        crate::profile::mark(7);"""),
    )
    instrumented = replace(
        instrumented,
        "        Arc::new(sys_native::SysOps::native()),\n        argv.clone(),\n        Some(bex_project::runtime_compiler()),",
        "        sys_ops,\n        argv.clone(),\n        runtime_compiler,",
    )
    old = _source_block("""        bex_engine::TelemetryRecording::from_boundary_env()
                .unwrap_or_else(|| {
                    bex_engine::TelemetryRecording::user_files(
                        btel_settings::publisher::RecordingConfig::default(),
                    )
                })
                .with_host("pack"),""")
    instrumented = replace(instrumented, old, "        recording,", expected=2).replace(
        "        bootstrap_argv,\n        Some(bex_project::runtime_compiler()),\n        btel_settings::clock::DEFAULT_MODE,\n        recording,",
        _source_block("""        bootstrap_argv,
            Some(bex_project::runtime_compiler()),
            btel_settings::clock::DEFAULT_MODE,
    """)
        + old,
    )
    instrumented = replace(
        instrumented,
        "    let Some(func_info) = engine.find_user_function",
        "    crate::profile::mark(8);\n    let Some(func_info) = engine.find_user_function",
    )
    instrumented = replace(
        instrumented,
        "    let raw_cli_tokens: &[String]",
        "    crate::profile::mark(9);\n    let raw_cli_tokens: &[String]",
    )
    instrumented = replace(
        instrumented,
        _source_block("""    finalize_dispatch(
            &engine,
            &target.qualified_name,
            parsed,
            envelope.output_format,
        )"""),
        _source_block("""    crate::profile::mark(10);
        let exit = finalize_dispatch(
            &engine,
            &target.qualified_name,
            parsed,
            envelope.output_format,
        );
        crate::profile::mark(20);
        exit"""),
    )
    instrumented = replace(
        instrumented,
        "    let json_args = match parsed.json_source",
        "    let _runtime_drop = crate::profile::RuntimeDrop;\n    let json_args = match parsed.json_source",
    )
    instrumented = replace(
        instrumented,
        "    let rt = match tokio::runtime::Runtime::new()",
        "    crate::profile::mark(11);\n    let rt = match tokio::runtime::Runtime::new()",
    )
    instrumented = replace(
        instrumented,
        "    let result = rt.block_on(dispatch_target",
        "    crate::profile::mark(12);\n    let result = rt.block_on(dispatch_target",
    )
    instrumented = replace(
        instrumented,
        "    engine.record_process_exit(match &result",
        "    crate::profile::mark(13);\n    engine.record_process_exit(match &result",
    )
    instrumented = replace(
        instrumented,
        "    rt.block_on(engine.shutdown());",
        "    crate::profile::mark(14);\n    rt.block_on(engine.shutdown());\n    crate::profile::mark(15);",
    )
    instrumented = replace(
        instrumented,
        "    let mut unhandled_spawn_failed = false;",
        "    crate::profile::mark(16);\n    let mut unhandled_spawn_failed = false;",
    )
    instrumented = replace(
        instrumented,
        "    match result {\n        Ok(DispatchResult::Ok)",
        "    crate::profile::mark(17);\n    let exit = match result {\n        Ok(DispatchResult::Ok)",
    )
    # Only the final return expression changes; all original automatic drops keep order.
    assert instrumented.endswith("    }\n}\n")
    instrumented = instrumented[: -len("    }\n}\n")] + _source_block("""    };
        crate::profile::mark(18);
        exit
    }
    """)
    profile_main = main
    profile_main = replace(
        profile_main,
        "    let envelope = libsui::find_section",
        "    profile::mark(1);\n    let envelope = libsui::find_section",
    )
    profile_main = replace(
        profile_main,
        "        .and_then(|section| {\n            baml_artifact::decode",
        "        .and_then(|section| {\n            profile::mark(2);\n            baml_artifact::decode",
    )
    profile_main = replace(
        profile_main,
        "    match envelope {\n        Ok(envelope) => baml_pack_host::run(envelope),",
        "    profile::mark(3);\n    let exit = match envelope {\n        Ok(envelope) => host::run(envelope),",
    )
    assert profile_main.endswith("    }\n}\n")
    profile_main = profile_main[: -len("    }\n}\n")] + _source_block("""    };
        profile::mark(21);
        profile::emit();
        exit
    }
    """)

    # Transform crate-level inner attributes before embedding the real library as a module.
    def module(source):
        source = source.replace(
            "#![allow(clippy::print_stdout, clippy::print_stderr, clippy::exit)]", ""
        )
        return "mod host {\n" + source + "\n}\n"

    (out / "profile.rs").write_text(profile + module(instrumented) + profile_main)
    (out / "control.rs").write_text(
        module(lib) + main.replace("baml_pack_host::run", "host::run")
    )

    fingerprints = args.release_dir.resolve() / ".fingerprint"

    def artifact(name, wanted=None, experimental=False):
        files = list(fingerprints.glob(f"{name}-*/lib-{name}.json"))
        if wanted is None:
            files = [
                p
                for p in files
                if ("startup-experiment" in json.loads(p.read_text())["features"])
                == experimental
            ]
            p = max(files, key=lambda p: p.stat().st_mtime)
        else:
            p = next(
                p
                for p in files
                if int.from_bytes(
                    bytes.fromhex(p.with_suffix("").read_text()), "little"
                )
                == wanted
            )
        return p, json.loads(p.read_text())

    selections = {}
    for revision in ("baseline", "candidate"):
        requested = getattr(args, f"{revision}_fingerprint")
        if requested:
            path = fingerprints / requested / "lib-baml_pack_host.json"
            host = json.loads(path.read_text())
        else:
            path, host = artifact(
                "baml_pack_host", experimental=revision == "candidate"
            )
        selected = {
            name: artifact(name, fp)
            for _, name, _, fp in host["deps"]
            if name != "build_script_build"
        }
        selections[revision] = {
            "host": str(path),
            "libraries": {name: str(p) for name, (p, _) in selected.items()},
        }
        for variant in ("control", "profile"):
            cmd = [
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
                f"dependency={args.release_dir.resolve()}/deps",
            ]
            for name, (fp, _) in selected.items():
                library = (
                    args.release_dir.resolve() / "deps" / f"lib{fp.parent.name}.rlib"
                )
                assert library.exists(), library
                cmd += ["--extern", f"{name}={library}"]
            print("Building", revision, variant, flush=True)
            subprocess.run(cmd, check=True)
    selections["sources"] = {
        str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in [
            root / "crates/baml_pack_host/src/lib.rs",
            root / "crates/baml_pack_host/src/main.rs",
        ]
    }
    (out / "build-metadata.json").write_text(json.dumps(selections, indent=2))
    (out / "native-empty.rs").write_text("fn main() {}\n")
    subprocess.run(
        [
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
            str(out / "native-empty.rs"),
            "-o",
            str(out / "native-empty"),
        ],
        check=True,
    )
    helper = (root / "scripts/btel-startup/measure.c").read_text()
    helper = helper.replace(
        "extern char **environ;",
        _source_block("""extern char **environ;
    static unsigned long long now_ns(void) {
        struct timespec t;
        clock_gettime(CLOCK_MONOTONIC, &t);
        return (unsigned long long)t.tv_sec * 1000000000ULL + (unsigned long long)t.tv_nsec;
    }"""),
    )
    helper = helper.replace(
        "double start = now(), first = -1;",
        "unsigned long long start_ns = now_ns();\n    double start = start_ns / 1e9, first = -1;",
    )
    helper = helper.replace(
        "double end = now();",
        "unsigned long long end_ns = now_ns();\n    double end = end_ns / 1e9;",
    )
    helper = helper.replace(
        'printf("{\\"wall_ms\\":%.6f',
        'printf("{\\"start_ns\\":%llu,\\"end_ns\\":%llu,\\"wall_ms\\":%.6f',
    )
    helper = helper.replace(
        "        (end-start)*1e3, first < 0",
        "        start_ns, end_ns, (end-start)*1e3, first < 0",
    )
    assert "start_ns, end_ns, (end-start)" in helper
    helper = helper.replace(
        "    struct rusage_info_v4 detailed",
        "    unsigned long long eof_ns = now_ns();\n    struct rusage_info_v4 detailed",
    )
    helper = helper.replace(
        "    struct rusage usage;",
        "    unsigned long long counters_end_ns = now_ns();\n    struct rusage usage;",
    )
    helper = helper.replace(
        '\\"start_ns\\":%llu,',
        '\\"start_ns\\":%llu,\\"eof_ns\\":%llu,\\"counters_end_ns\\":%llu,',
    )
    helper = helper.replace(
        "        start_ns, end_ns,",
        "        start_ns, eof_ns, counters_end_ns, end_ns,",
    )
    (out / "measure.c").write_text(helper)
    subprocess.run(
        ["/usr/bin/clang", "-O2", str(out / "measure.c"), "-o", str(out / "measure")],
        check=True,
    )


def measure(args):
    root = Path(__file__).resolve().parents[2]
    out = args.output.resolve()
    revisions = {
        name: getattr(args, name).resolve() for name in ("baseline", "candidate")
    }
    previous = args.fixtures.resolve()
    home = out / "home"
    bh = home / ".baml"
    bh.mkdir(parents=True, exist_ok=True)
    (bh / "config.toml").write_text("[update]\nauto_check = false\n")
    env = {
        "PATH": "/usr/bin:/bin",
        "LANG": "en_US.UTF-8",
        "NO_COLOR": "1",
        "HOME": str(home),
        "BAML_HOME": str(bh),
        "BAML_CLI_ALLOW_DIRECT": "1",
        "BAML_TOOLCHAIN": str(previous / "child"),
    }
    off = env | {"BAML_TELEMETRY": "off"}
    artifacts = {}
    for rev in ("baseline", "candidate"):
        compiler = revisions[rev] / "baml-cli"
        for var in ("control", "profile"):
            host = out / f"{rev}-{var}-host"
            assert host.exists(), host
            for case in ("empty", "wrapper"):
                project = (
                    previous / "projects/empty"
                    if case == "empty"
                    else root / "crates/baml/baml_src"
                )
                exe = out / f"{rev}-{var}-{case}"
                r = subprocess.run(
                    [
                        str(compiler),
                        "pack",
                        "Main",
                        "--project",
                        str(project),
                        "--host",
                        str(host),
                        "--output",
                        str(exe),
                    ],
                    env=off,
                    capture_output=True,
                    check=False,
                )
                if r.returncode:
                    raise RuntimeError(r.stdout + r.stderr)
                (out / f"{rev}-{var}-{case}-pack.log").write_bytes(r.stdout + r.stderr)
                artifacts[exe.name] = {
                    "bytes": exe.stat().st_size,
                    "sha256": hashlib.sha256(exe.read_bytes()).hexdigest(),
                }
    variants = [
        (rev, var, case, mode)
        for rev in ("baseline", "candidate")
        for var in ("original", "control", "profile")
        for case in ("empty", "wrapper")
        for mode in ("off", "local")
    ]
    variants += [
        ("floor", "native_empty", "empty", "off"),
        ("floor", "native_child", "wrapper", "off"),
        ("floor", "rust_wrapper", "wrapper", "off"),
    ]
    data = {"/".join(v): [] for v in variants}
    # Verify identical stdout separately; the parent helper intentionally consumes it.
    for rev in ("baseline", "candidate"):
        for case in ("empty", "wrapper"):
            outputs = []
            for var in ("original", "control", "profile"):
                exe = (
                    revisions[rev]
                    / ("empty-packed" if case == "empty" else "baml-packed")
                    if var == "original"
                    else out / f"{rev}-{var}-{case}"
                )
                result = subprocess.run(
                    [str(exe), *([] if case == "empty" else ["--version"])],
                    env=off,
                    capture_output=True,
                    check=True,
                )
                assert not result.stderr or (
                    var == "profile"
                    and result.stderr.startswith(b"BAML_STARTUP_PROFILE:")
                ), result.stderr
                outputs.append(result.stdout)
            assert outputs[0] == outputs[1] == outputs[2], (rev, case, outputs)

    def take_sample(rev, var, case, mode):
        if rev == "floor":
            exe = {
                "native_empty": out / "native-empty",
                "native_child": previous / "child",
                "rust_wrapper": args.rust_wrapper.resolve(),
            }[var]
        elif var == "original":
            exe = revisions[rev] / (
                "empty-packed" if case == "empty" else "baml-packed"
            )
        else:
            exe = out / f"{rev}-{var}-{case}"
        child_args = [] if case == "empty" else ["--version"]
        r = subprocess.run(
            [str(out / "measure"), str(exe), *child_args],
            env=env | ({"BAML_TELEMETRY": "off"} if mode == "off" else {}),
            capture_output=True,
            check=True,
        )
        sample = json.loads(r.stdout)
        assert sample["exit"] == 0, sample
        sample["cpu_ms"] = sample["user_ms"] + sample["system_ms"]
        if var == "profile":
            prefix = b"BAML_STARTUP_PROFILE:"
            assert r.stderr.startswith(prefix) and r.stderr.count(prefix) == 1, r.stderr
            marks = json.loads(r.stderr[len(prefix) :])
            sample["marks"] = marks
            ordered = [
                ("parent_start", sample["start_ns"]),
                *marks.items(),
                ("stdout_eof", sample["eof_ns"]),
                ("counters_end", sample["counters_end_ns"]),
                ("parent_end", sample["end_ns"]),
            ]
            assert all(a[1] <= b[1] for a, b in pairwise(ordered)), ordered
            sample["phases_ms"] = {
                b[0]: (b[1] - a[1]) / 1e6 for a, b in pairwise(ordered)
            }
            assert abs(sum(sample["phases_ms"].values()) - sample["wall_ms"]) < 0.001
        else:
            assert not r.stderr, r.stderr
        return sample

    for _ in range(3):
        for v in variants:
            take_sample(*v)
    randomizer = random.Random(2026100402)
    for n in range(args.runs):
        randomizer.shuffle(variants)
        for v in variants:
            data["/".join(v)].append(take_sample(*v))
        if (n + 1) % 20 == 0:
            print(f"{n + 1}/{args.runs} rounds", flush=True)

    def summarize(samples):
        scalar = [
            key
            for key in samples[0]
            if key
            not in (
                "marks",
                "phases_ms",
                "start_ns",
                "end_ns",
                "eof_ns",
                "counters_end_ns",
            )
        ]
        result = {}
        for key in scalar:
            values = sorted(s[key] for s in samples)
            result[key] = {
                "median": statistics.median(values),
                "p95": values[(len(values) * 95 + 99) // 100 - 1],
                "min": values[0],
                "max": values[-1],
            }
        if "phases_ms" in samples[0]:
            result["phases_ms"] = {}
            for key in samples[0]["phases_ms"]:
                values = sorted(s["phases_ms"][key] for s in samples)
                result["phases_ms"][key] = {
                    "median": statistics.median(values),
                    "p95": values[(len(values) * 95 + 99) // 100 - 1],
                    "mean": statistics.mean(values),
                }
        return result

    summary = {k: summarize(v) for k, v in data.items()}
    (out / "results.json").write_text(
        json.dumps(
            {
                "summary": summary,
                "raw": data,
                "artifacts": artifacts,
                "seed": 2026100402,
                "runs": args.runs,
                "warmups": 3,
            },
            indent=2,
        )
    )
    for name, values in summary.items():
        print(
            name,
            round(values["wall_ms"]["median"], 3),
            round(values["cpu_ms"]["median"], 3),
            flush=True,
        )
    for name, values in summary.items():
        if "/profile/" in name:
            print(name, json.dumps(values["phases_ms"], indent=2), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    build_parser = commands.add_parser("build")
    build_parser.add_argument("output", type=Path)
    build_parser.add_argument(
        "--release-dir", type=Path, default=Path("target/release")
    )
    build_parser.add_argument("--baseline-fingerprint")
    build_parser.add_argument("--candidate-fingerprint")
    measure_parser = commands.add_parser("measure")
    measure_parser.add_argument("output", type=Path)
    measure_parser.add_argument("--baseline", type=Path, required=True)
    measure_parser.add_argument("--candidate", type=Path, required=True)
    measure_parser.add_argument("--fixtures", type=Path, required=True)
    measure_parser.add_argument("--rust-wrapper", type=Path, required=True)
    measure_parser.add_argument("--runs", type=int, default=100)
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error(
            "This diagnostic uses macOS libproc, CLOCK_MONOTONIC=6 and Mach-O constructors."
        )
    if args.command == "build":
        build(args)
    else:
        if args.runs < 1:
            parser.error("--runs must be positive")
        measure(args)


if __name__ == "__main__":
    main()
