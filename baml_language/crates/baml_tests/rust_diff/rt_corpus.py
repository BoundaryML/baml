#!/usr/bin/env python3
"""Run the shared BAML test corpus against VM, RT and RT GC stress.

Only explicit admission diagnostics are unsupported. Build errors and missing
reports fail the gate. All process evidence stays in the Cargo target directory.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys

HERE = Path(__file__).resolve().parent
CATEGORIES = ("match", "mismatch", "unsupported", "crash/timeout", "excluded")
# These exclusions concern observable nondeterminism in the VM as well. Merely
# using random values or time internally does not qualify a test for exclusion.
EXCLUSIONS = {
    # Exact canonical IDs, with an evidence-backed reason, belong here.
}


def invoke(argv, *, env, cwd, timeout, evidence):
    evidence.parent.mkdir(parents=True, exist_ok=True)
    with evidence.with_suffix(".stdout").open("w") as out, evidence.with_suffix(".stderr").open("w") as err:
        child = subprocess.Popen([str(x) for x in argv], cwd=cwd, env=env, stdout=out, stderr=err, start_new_session=True)
        timed_out = False
        try:
            code = child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(child.pid, signal.SIGKILL)
            code = child.wait()
    return {"code": code, "timeout": timed_out,
            "stdout": evidence.with_suffix(".stdout").read_text(errors="replace"),
            "stderr": evidence.with_suffix(".stderr").read_text(errors="replace")}


def marker(result, prefix):
    rows = [json.loads(line[len(prefix):]) for line in result["stdout"].splitlines() if line.startswith(prefix)]
    if len(rows) != 1:
        raise RuntimeError(f"expected one {prefix.strip()} report, found {len(rows)}; exit={result['code']}; {result['stderr'][-2000:]}")
    return rows[0]


def vm_leaf(result, name):
    if result["timeout"] or result["code"] not in (0, 2):
        return None
    report = marker(result, "BAML_CORPUS_REPORT\t")
    if result["code"] == 2 and report.get("outcome") == "pass":
        return None  # An unhandled spawned task failed after the leaf report.
    selected = report["passed"] + report["failed"] + report["tolerated"]
    if selected != [name] or report["total"] != 1:
        raise RuntimeError(f"VM selector did not select exactly {name}: {report}")
    output = "".join(line for line in result["stdout"].splitlines(keepends=True)
                     if not line.startswith("BAML_CORPUS_REPORT\t"))
    return {"outcome": "pass" if name in report["passed"] else "tolerated" if name in report["tolerated"] else "fail",
            "messages": sorted(report["messages"]), "output": output}


def rt_leaf(result, name):
    if result["timeout"] or result["code"] not in (0, 2, 3):
        return None
    try:
        report = marker(result, "BAML_CORPUS_RESULT\t")
    except RuntimeError:
        return None
    if report["name"] != name:
        raise RuntimeError(f"wrong RT test identity: {report}")
    output = "".join(line for line in result["stdout"].splitlines(keepends=True) if not line.startswith("BAML_CORPUS_RESULT\t"))
    return {"outcome": report["outcome"], "messages": [] if report["message"] is None else [report["message"]], "output": output}


def regressions(before, after):
    failures = []
    for name, old in before["tests"].items():
        new = after["tests"].get(name)
        if new is None:
            failures.append(f"test disappeared: {name}")
        elif old["category"] == "match" and new["category"] != "match":
            failures.append(f"match regressed to {new['category']}: {name}")
        elif old["category"] != "excluded" and new["category"] == "excluded":
            failures.append(f"new exclusion requires review: {name}")
    for name, new in after["tests"].items():
        old = before["tests"].get(name, {})
        if new["category"] in ("mismatch", "crash/timeout") and (old.get("category"), old.get("fingerprint", old.get("signature"))) != (new["category"], new.get("fingerprint", new.get("signature"))):
            failures.append(f"new {new['category']}: {name}: {new.get('signature')}")
    return failures


def summarize(report):
    counts = Counter(row["category"] for row in report["tests"].values())
    print("\nmode                 match mismatch unsupported crash/timeout excluded")
    for mode in ("rt", "rt-gc-stress"):
        per_mode = Counter()
        for row in report["tests"].values():
            category = row["category"]
            if category not in ("unsupported", "excluded"):
                oracle, native = row.get("vm", {}), row.get(mode, {})
                if "outcome" not in oracle or "outcome" not in native or native.get("outcome") == "crash":
                    category = "crash/timeout"
                else:
                    category = "match" if oracle == native else "mismatch"
            per_mode[category] += 1
        print(f"{mode:17} {per_mode['match']:8} {per_mode['mismatch']:8} {per_mode['unsupported']:11} {per_mode['crash/timeout']:13} {per_mode['excluded']:8}")
    print(f"{'combined':17} {counts['match']:8} {counts['mismatch']:8} {counts['unsupported']:11} {counts['crash/timeout']:13} {counts['excluded']:8}")
    for category in ("unsupported", "mismatch", "crash/timeout", "excluded"):
        groups = defaultdict(list)
        for name, row in report["tests"].items():
            if row["category"] == category:
                groups[row["signature"]].append(name)
        for signature, names in sorted(groups.items(), key=lambda item: (-len(item[1]), item[0])):
            print(f"{category}\t{len(names)}\t{signature}\t{', '.join(names[:3])}")
    for name, row in report["tests"].items():
        if row["category"] in ("mismatch", "crash/timeout"):
            print(f"BUG {name}\n  VM: {json.dumps(row.get('vm'), ensure_ascii=False)}\n  rt: {json.dumps(row.get('rt'), ensure_ascii=False)}\n  rt-gc-stress: {json.dumps(row.get('rt-gc-stress'), ensure_ascii=False)}")
    report["counts"] = {key: counts[key] for key in CATEGORIES}


def oracle_fingerprint(workspace, env):
    """Reuse VM observations only when its complete source input is unchanged.

    The RT emitter, RT runtime and the RT-only builtin emitter cannot execute in
    an interpreter invocation. All frontend/VM/stdlib/corpus sources stay hashed.
    """
    digest = hashlib.sha256()
    ignored_crates = {"baml_compiler2_rust", "baml_rt", "baml_rt_gc", "baml_rt_tasks", "baml_rt_island"}
    for root in (workspace / "crates", workspace / "sdks", workspace / "forks", workspace / "vendor"):
        for path in sorted(root.rglob("*")):
            if not path.is_file() or any(part in ("target", "node_modules", ".git", "__pycache__") for part in path.relative_to(root).parts):
                continue
            relative = path.relative_to(workspace)
            if len(relative.parts) > 1 and relative.parts[0] == "crates":
                crate = relative.parts[1]
                if crate in ignored_crates:
                    continue
                if crate == "baml_tests" and "baml_src" not in relative.parts:
                    continue
                if crate == "baml_builtins2_codegen" and path.name.startswith("codegen_rt"):
                    continue
            digest.update(str(relative).encode() + b"\0" + path.read_bytes())
    for config in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml"):
        digest.update((workspace / config).read_bytes())
    digest.update(subprocess.check_output(["cargo", "--version"], cwd=workspace, env=env))
    digest.update(env.get("CARGO_PROFILE_DEV_DEBUG", "").encode())
    digest.update(env.get("BAML_TELEMETRY", "").encode())
    return digest.hexdigest()


def cached_invoke(argv, *, cache, **kwargs):
    if cache.exists():
        return json.loads(cache.read_text())
    result = invoke(argv, **kwargs)
    if not result["timeout"] and result["code"] in (0, 2) and any(line.startswith("BAML_CORPUS_REPORT\t") for line in result["stdout"].splitlines()):
        cache.parent.mkdir(parents=True, exist_ok=True)
        cache.write_text(json.dumps(result) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace", type=Path, default=HERE.parents[2])
    parser.add_argument("--label", help="separate evidence label for source-snapshot audits")
    parser.add_argument("--baseline", type=Path, default=HERE / "rt-corpus-baseline.json")
    parser.add_argument("--update-baseline", action="store_true")
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--report-only", action="store_true", help="audit a different worktree; still rejects infrastructure failures")
    parser.add_argument("--select", help="diagnostic subset; cannot update or check the baseline")
    args = parser.parse_args()
    workspace = args.workspace.resolve()
    target = Path(os.environ.get("CARGO_TARGET_DIR", workspace.parent / "target-corpus")).resolve()
    label = args.label or workspace.parent.name
    if Path(label).name != label: raise RuntimeError("label must be one path component")
    scratch = target / "rt-corpus" / label
    scratch.mkdir(parents=True, exist_ok=True)
    home = scratch / "home"
    home.mkdir(exist_ok=True)
    (home / "config.toml").write_text("[update]\nauto_check = false\n")
    env = dict(os.environ, CARGO_BUILD_JOBS="6", CARGO_TARGET_DIR=str(target), BAML_CLI_ALLOW_DIRECT="1",
               BAML_HOME=str(home), BAML_TELEMETRY="medium", NO_COLOR="1", RUST_BACKTRACE="0")
    env.pop("BAML_GC_STRESS", None)
    env.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
    cli = target / "debug" / "baml-cli"
    timeout = int(env.get("BAML_RT_CORPUS_TIMEOUT_SECS", "120"))
    if not args.skip_build:
        print(f"Building CLI once: {workspace}", flush=True)
        build = invoke(["cargo", "build", "-p", "baml_cli"], env=env, cwd=workspace, timeout=7200, evidence=scratch / "build")
        if build["code"] or build["timeout"]:
            raise RuntimeError(f"CLI build failed: {build['stderr'][-6000:]}")
    corpus = workspace / "crates/baml_tests/baml_src"
    command = [cli, "--agent-skill-check", "off", "test", "--from", corpus, "--corpus-report", "--max-concurrency", "1", "--log", "trace", "--color", "never", "--no-progress"]
    selectors = [] if args.select is None else ["-i", args.select]
    exclusions = dict(EXCLUSIONS)
    if args.baseline.exists():
        exclusions.update({name: row["signature"] for name, row in json.loads(args.baseline.read_text())["tests"].items()
                           if row["category"] == "excluded"})
    for name, reason in exclusions.items():
        selectors.extend(["-x", name])
        print(f"EXCLUDED {name}: {reason}")
    print("PROFILE EXCLUSIONS (both sides): ::live:: (provider/network credentials); root.fixtures. (compile-only fixtures)", flush=True)
    print("Running the existing VM corpus command", flush=True)
    full_command = list(command)
    full_command[full_command.index("--max-concurrency") + 1] = "6"
    fingerprint = oracle_fingerprint(workspace, env)
    oracle_cache = target / "rt-corpus-oracles" / fingerprint
    selection_key = hashlib.sha256(json.dumps(selectors).encode()).hexdigest()[:20]
    reference = cached_invoke(full_command + selectors, cache=oracle_cache / f"corpus-{selection_key}.json",
                              env=env, cwd=workspace, timeout=3600, evidence=scratch / "vm-corpus")
    if reference["timeout"] or reference["code"] not in (0, 2):
        raise RuntimeError(f"VM corpus failed before producing a reference: {reference['stderr'][-5000:]}")
    vm = marker(reference, "BAML_CORPUS_REPORT\t")
    vm_names = vm["passed"] + vm["failed"] + vm["tolerated"]
    if not vm_names or len(set(vm_names)) != len(vm_names) or len(vm_names) != vm["total"]:
        raise RuntimeError(f"invalid VM test inventory: total={vm['total']}, names={len(vm_names)}")
    binary = scratch / "tests-rt"
    binary.unlink(missing_ok=True)
    pack_dir = scratch / "pack"
    pack_dir.mkdir(exist_ok=True)
    shared_native_target = target / "rt-corpus-native"
    shared_native_target.mkdir(exist_ok=True)
    if not (pack_dir / "target").exists():
        (pack_dir / "target").symlink_to(shared_native_target, target_is_directory=True)
    print(f"Packing {len(vm_names)} VM tests with per-leaf RT admission", flush=True)
    pack = invoke([cli, "--agent-skill-check", "off", "pack", "--tests", "--rust", "--rt", "--from", corpus, "--rust-profile", "fast", "--rust-workspace", workspace, "--rust-build-dir", scratch / "pack", "-o", binary],
                  env=env, cwd=workspace, timeout=7200, evidence=scratch / "pack")
    if pack["code"] or pack["timeout"]:
        raise RuntimeError(f"RT corpus pack failed: {pack['stderr'][-7000:]}")
    listing = invoke([binary, "--list"], env=env, cwd=workspace, timeout=timeout, evidence=scratch / "rt-inventory")
    if listing["code"] or listing["timeout"]:
        raise RuntimeError(f"RT inventory failed: {listing}")
    inventory = json.loads(listing["stdout"])
    inventory = {row["name"]: row["admission"] for row in inventory}
    missing = set(vm_names) - inventory.keys()
    if missing:
        raise RuntimeError(f"RT inventory lost VM tests: {sorted(missing)}")
    report = {"schema": 1, "workspace": str(workspace), "vm_source_fingerprint": fingerprint, "tests": {}, "vm_total": len(vm_names)}
    for index, name in enumerate(sorted(vm_names)):
        admission = inventory[name]
        vm_outcome = "pass" if name in vm["passed"] else "tolerated" if name in vm["tolerated"] else "fail"
        if admission is not None:
            category = "unsupported" if "unsupported in baml_rt:" in admission else "crash/timeout"
            report["tests"][name] = {"category": category, "signature": admission, "vm": {"outcome":vm_outcome}}
            if category == "crash/timeout":
                report["tests"][name].update({mode: {"outcome":"crash", "message": admission} for mode in ("rt", "rt-gc-stress")})
            continue
        key = hashlib.sha256(name.encode()).hexdigest()[:20]
        leaf_selectors = ["--corpus-exact", name]
        vm_result = cached_invoke(command + leaf_selectors, cache=oracle_cache / f"leaf-{key}.json",
                                  env=env, cwd=workspace, timeout=timeout, evidence=scratch / "leaves" / f"{key}-vm")
        oracle = vm_leaf(vm_result, name)
        row = {"vm": oracle or vm_result}
        category = "match"
        signatures = []
        for mode in ("rt", "rt-gc-stress"):
            native_env = dict(env)
            if mode == "rt-gc-stress":
                native_env["BAML_GC_STRESS"] = "1"
            native_result = invoke([binary, "--exact-test", name], env=native_env, cwd=workspace, timeout=timeout, evidence=scratch / "leaves" / f"{key}-{mode}")
            native = rt_leaf(native_result, name)
            row[mode] = native or native_result
            if oracle is None or native is None:
                category = "crash/timeout"
                signatures.append(f"{mode}: VM exit {vm_result['code']}, RT exit {native_result['code']}, timeout {vm_result['timeout'] or native_result['timeout']}")
            elif native != oracle:
                if category != "crash/timeout": category = "mismatch"
                signatures.append(f"{mode}: VM {oracle['outcome']} / RT {native['outcome']}; " + "; ".join(field for field in ("messages", "output") if oracle[field] != native[field]))
        if category == "mismatch" and oracle is not None:
            repeat_result = invoke(command + leaf_selectors, env=env, cwd=workspace, timeout=timeout,
                                   evidence=scratch / "leaves" / f"{key}-vm-repeat")
            repeat = vm_leaf(repeat_result, name)
            if repeat is not None and repeat != oracle:
                category = "excluded"
                signatures = ["VM nondeterminism: two isolated runs differ in " + ", ".join(field for field in oracle if oracle[field] != repeat[field])]
                row["vm-repeat"] = repeat
                print(f"EXCLUDED {name}: {signatures[0]}", flush=True)
        if category in ("mismatch", "crash/timeout"):
            row["fingerprint"] = hashlib.sha256(json.dumps(row, sort_keys=True).encode()).hexdigest()
        row.update(category=category, signature=" | ".join(signatures))
        report["tests"][name] = row
        print(f"[{index+1}/{len(vm_names)}] {category}: {name}", flush=True)
    for name, reason in exclusions.items():
        report["tests"][name] = {"category":"excluded", "signature":reason}
    summarize(report)
    output = scratch / "report.json"
    output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"Evidence: {output}")
    if args.select:
        if args.update_baseline: raise RuntimeError("a subset cannot update the full baseline")
        return 0 if all(row["category"] not in ("mismatch", "crash/timeout") for row in report["tests"].values()) else 1
    if args.update_baseline:
        baseline = {"schema": 1, "counts":report["counts"], "tests": {name: {key: row[key] for key in ("category", "signature", "fingerprint") if key in row} for name,row in report["tests"].items()}}
        args.baseline.write_text(json.dumps(baseline, indent=2, sort_keys=True) + "\n")
        return 0
    if args.report_only:
        return 0
    if not args.baseline.exists(): raise RuntimeError("baseline missing; run --update-baseline after inspecting every mismatch/crash")
    failures = regressions(json.loads(args.baseline.read_text()), report)
    for failure in failures: print(f"REGRESSION {failure}", file=sys.stderr)
    return bool(failures)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (RuntimeError, ValueError) as error:
        print(f"CORPUS GATE FAILED: {error}", file=sys.stderr)
        sys.exit(1)
