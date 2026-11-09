#!/usr/bin/env python3
"""Run the shared BAML test corpus against VM, RT and RT GC stress.

Only explicit admission diagnostics are unsupported. Build errors and missing
reports fail the gate. All process evidence stays in the Cargo target directory.
"""
from __future__ import annotations

import argparse
import bisect
from collections import Counter, defaultdict
import hashlib
import json
import os
from pathlib import Path
import signal
import shutil
import re
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
CATEGORIES = ("match", "mismatch", "unsupported", "crash/timeout", "excluded")
# These exclusions concern observable nondeterminism in the VM as well. Merely
# using random values or time internally does not qualify a test for exclusion.
EXCLUSIONS = {
    # Exact canonical IDs, with an evidence-backed reason, belong here.
}


def write_json(path, value, **formatting):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    temporary.write_text(json.dumps(value, **formatting) + "\n")
    temporary.replace(path)


def restore_native_compilation_entries(admission, originals):
    # Native sources can change without changing the CLI. Retry previously
    # broken generated modules on each invocation, retaining compiler admission.
    for backup in originals.glob("*.json"):
        record = json.loads(backup.read_text())
        cache = (admission / record["cache_file"]).resolve()
        cache.relative_to(admission.resolve())
        if cache.exists() and json.loads(cache.read_text()).get("Err", "").startswith("RT native compilation failed:"):
            write_json(cache, record["original"])


def isolate_native_compilation_errors(stderr, source, admission, originals):
    """Attribute generated-module diagnostics; shared/entry-host errors stay fatal."""
    if not source.exists():
        return []
    text = source.read_text()
    lines = text.splitlines()
    modules = [(i + 1, int(match[1])) for i, line in enumerate(lines)
               if (match := re.match(r"^mod test_(\d+) \{", line))]
    if not modules:
        return []
    starts = [line for line, _ in modules]
    end = next((i + 1 for i, line in enumerate(lines)
                if i + 1 > starts[-1] and line.startswith("fn main(")), len(lines) + 1)
    plain = re.sub(r"\x1b\[[0-9;]*m", "", stderr)
    headers = list(re.finditer(r"(?m)^error(?:\[[^\]]+\])?: [^\n]+", plain))
    failures = defaultdict(set)
    for index, header in enumerate(headers):
        message = header.group()
        if message.startswith(("error: could not compile", "error: aborting due", "error: cargo build")):
            continue
        block = plain[header.end():headers[index + 1].start() if index + 1 < len(headers) else len(plain)]
        location = re.search(r"(?m)^\s*-->\s+(.+):(\d+):(\d+)\s*$", block)
        if location is None:
            return []
        filename, line = location[1], int(location[2])
        reported = Path(filename)
        if not reported.is_absolute():
            reported = source.parent.parent / reported
        if reported.resolve() != source.resolve() or line < 1 or line > len(lines):
            return []
        module = bisect.bisect_right(starts, line) - 1
        if module < 0 or line >= end or lines[line - 1].lstrip().startswith(("fn entry(", "pub fn run(")):
            return []
        failures[modules[module][1]].add(message)
    pending = []
    for entry, messages in failures.items():
        candidates = []
        for cache in admission.rglob(f"{entry}.json"):
            record = json.loads(cache.read_text())
            if "Ok" in record and record["Ok"] in text:
                candidates.append((cache, record))
        if not candidates:
            return []
        reason = "RT native compilation failed: " + "; ".join(sorted(messages))
        pending.append((entry, reason, candidates))
    for entry, reason, candidates in pending:
        for cache, record in candidates:
            relative = str(cache.relative_to(admission))
            backup = originals / (hashlib.sha256(relative.encode()).hexdigest() + ".json")
            write_json(backup, {"cache_file": relative, "original": record})
            write_json(cache, {"Err": reason})
    return [(entry, reason) for entry, reason, _ in pending]


def invoke(argv, *, env, cwd, timeout, evidence):
    evidence.parent.mkdir(parents=True, exist_ok=True)
    with evidence.with_suffix(".stdout").open("w") as out, evidence.with_suffix(".stderr").open("w") as err:
        child = subprocess.Popen([str(x) for x in argv], cwd=cwd, env=env, stdout=out, stderr=err, start_new_session=True)
        timed_out = False
        admission_timeout = None
        admission_crash = None
        watched = "pack" in [str(x) for x in argv] and "BAML_RT_CORPUS_ADMISSION_CACHE" in env
        if watched:
            start = time.monotonic()
            active, active_start = None, None
            position = 0
            limit = int(env.get("BAML_RT_CORPUS_TIMEOUT_SECS", "120"))
            with evidence.with_suffix(".stderr").open(errors="surrogateescape") as progress:
                while True:
                    progress.seek(position)
                    while True:
                        line_start = progress.tell()
                        line = progress.readline()
                        if not line.endswith("\n"):
                            progress.seek(line_start)
                            break
                        if line.startswith("BAML_CORPUS_ADMISSION\t"):
                            item = json.loads(line.split("\t", 1)[1])
                            if item["phase"] == "start":
                                active, active_start = item, time.monotonic()
                            elif active is not None and item["entry"] == active["entry"]:
                                active, active_start = None, None
                    position = progress.tell()
                    if child.poll() is not None:
                        break
                    now = time.monotonic()
                    if active is not None and now - active_start > limit:
                        admission_timeout = active
                        os.killpg(child.pid, signal.SIGKILL)
                        break
                    if now - start > timeout:
                        timed_out = True
                        os.killpg(child.pid, signal.SIGKILL)
                        break
                    time.sleep(0.1)
            code = child.wait()
            if code and active is not None and admission_timeout is None and not timed_out:
                admission_crash = active
        else:
            try:
                code = child.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
                os.killpg(child.pid, signal.SIGKILL)
                code = child.wait()
    return {"code": code, "timeout": timed_out, "admission_timeout": admission_timeout,
            "admission_crash": admission_crash,
            "stdout": evidence.with_suffix(".stdout").read_bytes().decode("utf-8", "surrogateescape"),
            "stderr": evidence.with_suffix(".stderr").read_bytes().decode("utf-8", "surrogateescape")}


def marker_parts(result, prefix):
    # Programs can write without a final newline, or print the marker literally.
    # The host appends its report last; remove only that JSON record and newline.
    position = result["stdout"].rfind(prefix)
    try:
        if position < 0:
            raise ValueError("marker missing")
        payload = result["stdout"][position + len(prefix):]
        report, end = json.JSONDecoder().raw_decode(payload)
        if not isinstance(report, dict):
            raise ValueError("report must be an object")
    except ValueError as error:
        raise RuntimeError(f"invalid {prefix.strip()} report; exit={result['code']}; {result['stderr'][-2000:]}") from error
    tail = payload[end:]
    if tail.startswith("\r\n"):
        tail = tail[2:]
    elif tail.startswith("\n"):
        tail = tail[1:]
    return report, result["stdout"][:position] + tail


def marker(result, prefix):
    return marker_parts(result, prefix)[0]


def vm_program_stderr(result, report):
    stderr = result["stderr"]
    if report.get("stream_protocol", 1) >= 2:
        return stderr
    # Saved CLIs from protocol 1 append exactly one terminal status record.
    # Remove that record, never identically worded program lines before it.
    passed, failed, tolerated = (len(report[key]) for key in ("passed", "failed", "tolerated"))
    total = report["total"]
    if tolerated:
        failures = "failure" if tolerated == 1 else "failures"
        if result["code"] == 2:
            summary = f"{passed} passed, {failed} failed, {tolerated} tolerated {failures}, {total} total"
        else:
            summary = f"aggregate passed — {passed} passed, {tolerated} tolerated {failures}, {total} total"
    else:
        summary = f"{passed} passed, {failed} failed, {total} total"
    escaped = re.escape(summary)
    pattern = rf"(?m)^[ \t]*(?:Finished {escaped}(?: in [^\r\n]+)?|(?:[Ee]rror: )?test failures — {escaped})\r?\n?\Z"
    terminal = re.search(pattern, stderr)
    return stderr[:terminal.start()] if terminal else stderr


def vm_leaf(result, name):
    if result["timeout"] or result["code"] not in (0, 2):
        return None
    try:
        report, output = marker_parts(result, "BAML_CORPUS_REPORT\t")
    except RuntimeError:
        return None
    if result["code"] == 2 and report.get("outcome") == "pass":
        return None  # An unhandled spawned task failed after the leaf report.
    selected = report["passed"] + report["failed"] + report["tolerated"]
    if selected != [name] or report["total"] != 1:
        raise RuntimeError(f"VM selector did not select exactly {name}: {report}")
    return {"outcome": "pass" if name in report["passed"] else "tolerated" if name in report["tolerated"] else "fail",
            "messages": sorted(report["messages"]), "output": output, "stderr": vm_program_stderr(result, report)}


def rt_leaf(result, name):
    if result["timeout"] or result["code"] not in (0, 2, 3):
        return None
    try:
        report, output = marker_parts(result, "BAML_CORPUS_RESULT\t")
    except RuntimeError:
        return None
    if report["name"] != name:
        raise RuntimeError(f"wrong RT test identity: {report}")
    return {"outcome": report["outcome"], "messages": [] if report["message"] is None else [report["message"]],
            "output": output, "stderr": result["stderr"]}


def admission_category(error):
    # A compiler panic mentioning an unsupported construct remains a crash.
    return "unsupported" if error.startswith("unsupported in baml_rt:") else "crash/timeout"


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


def normalize_admission_signature(signature):
    """Group diagnostics by blocker; retain raw errors for evidence/comparison."""
    # Reachability/emission coordinates identify the caller, not the blocker.
    signature = re.split(r" \((?:constructor in |reaching |emitting |layout |[^\n]*? local \d+:|interface receiver )", signature, maxsplit=1)[0]
    signature = re.sub(r"\b0x[0-9a-fA-F]+\b", "<ptr>", signature)
    signature = re.sub(r"\b(TypeTag|TypeId|ObjectIndex|FileId|Ptr|ListPtr|MapStrPtr|ListClosure|ListFuture|Dyn|Enum|Type|Local|BlockId)\(\d+\)", r"\1(<id>)", signature)
    signature = re.sub(r"\bSourceRoot\(Id\([0-9a-fA-F]+\)\)", "SourceRoot(<id>)", signature)
    signature = re.sub(r"\b(type_id|type_tag|object_id|object_index)(\s*[:=]\s*)\d+\b", r"\1\2<id>", signature)
    return " ".join(signature.split())


def admission_burndown(report):
    groups = {}
    for name, row in report["tests"].items():
        if row["category"] != "unsupported":
            continue
        # Today admission stops at its first error. If a capture has multiple
        # observed errors, a leaf belongs to each group but is sole-blocked only
        # when those observations normalize to one signature.
        signatures = {normalize_admission_signature(error)
                      for error in (row.get("admission_errors") or [row["signature"]])}
        for signature in signatures:
            group = groups.setdefault(signature, {"signature": signature, "count": 0, "only_estimate": 0, "examples": []})
            group["count"] += 1
            group["only_estimate"] += len(signatures) == 1
            group["examples"].append(name)
    for group in groups.values():
        group["examples"] = sorted(group["examples"])[:3]
    return sorted(groups.values(), key=lambda group: (-group["count"], group["signature"]))


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
    groups = admission_burndown(report)
    print(f"ADMISSION BURN-DOWN: top {min(30, len(groups))} of {len(groups)} normalized groups")
    print("tests\tonly (first-error estimate)\tadmission signature\texample tests")
    for group in groups[:30]:
        print(f"{group['count']}\t{group['only_estimate']}\t{group['signature']}\t{', '.join(group['examples'])}")
    print("Only is an upper bound from observed first errors; fixing a blocker can expose another.")
    for category in ("mismatch", "crash/timeout", "excluded"):
        groups = defaultdict(list)
        for name, row in report["tests"].items():
            if row["category"] == category:
                groups[row["signature"]].append(name)
        for signature, names in sorted(groups.items(), key=lambda item: (-len(item[1]), item[0])):
            print(f"{category}\t{len(names)}\t{signature}\t{', '.join(names[:3])}")
    for name, row in report["tests"].items():
        if row["category"] in ("mismatch", "crash/timeout"):
            print(f"BUG {name}\n  VM: {json.dumps(row.get('vm'))}\n  rt: {json.dumps(row.get('rt'))}\n  rt-gc-stress: {json.dumps(row.get('rt-gc-stress'))}")
    report["counts"] = {key: counts[key] for key in CATEGORIES}


def oracle_fingerprint(workspace, env):
    """Reuse VM observations only when its complete source input is unchanged.

    The RT emitter, RT runtime and the RT-only builtin emitter cannot execute in
    an interpreter invocation. All frontend/VM/stdlib/corpus sources stay hashed.
    """
    # Capture-format changes must not reuse observations from a lossy decoder.
    digest = hashlib.sha256(b"rt-corpus-vm-evidence-v2-lossless-streams")
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
    if not result["timeout"] and result["code"] in (0, 2) and "BAML_CORPUS_REPORT\t" in result["stdout"]:
        write_json(cache, result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace", type=Path, default=HERE.parents[2])
    parser.add_argument("--label", help="separate evidence label for source-snapshot audits")
    parser.add_argument("--baseline", type=Path, default=HERE / "rt-corpus-baseline.json")
    parser.add_argument("--update-baseline", action="store_true")
    parser.add_argument("--skip-build", action="store_true")
    parser.add_argument("--cli", type=Path, help="reuse this built CLI instead of building it")
    parser.add_argument("--report-only", action="store_true", help="audit a different worktree; still rejects infrastructure failures")
    parser.add_argument("--select", help="diagnostic subset; cannot update or check the baseline")
    parser.add_argument("--summarize-report", type=Path, help="print a saved report without rebuilding or running tests")
    args = parser.parse_args()
    if args.summarize_report:
        if args.update_baseline:
            raise RuntimeError("a saved report cannot update the baseline; run the gate")
        summarize(json.loads(args.summarize_report.read_text()))
        return 0
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
    source_cli = args.cli.resolve() if args.cli else target / "debug" / "baml-cli"
    timeout = int(env.get("BAML_RT_CORPUS_TIMEOUT_SECS", "120"))
    if not args.skip_build and args.cli is None:
        print(f"Building CLI once: {workspace}", flush=True)
        build = invoke(["cargo", "build", "-p", "baml_cli"], env=env, cwd=workspace, timeout=7200, evidence=scratch / "build")
        if build["code"] or build["timeout"]:
            raise RuntimeError(f"CLI build failed: {build['stderr'][-6000:]}")
    cli = scratch / "baml-cli"
    if source_cli != cli:
        shutil.copy2(source_cli, cli)
    corpus = workspace / "crates/baml_tests/baml_src"
    command = [cli, "--agent-skill-check", "off", "test", "--from", corpus, "--corpus-report", "--max-concurrency", "1", "--log", "trace", "--color", "never", "--no-progress"]
    selectors = [] if args.select is None else ["-i", args.select]
    env.pop("BAML_RT_CORPUS_SELECT", None)
    if args.select is not None:
        env["BAML_RT_CORPUS_SELECT"] = args.select
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
    env["BAML_RT_CORPUS_ADMISSION_CACHE"] = str(scratch / "admission" / hashlib.sha256(cli.read_bytes()).hexdigest())
    native_originals = scratch / "native-compile-originals"
    restore_native_compilation_entries(scratch / "admission", native_originals)
    pack_command = [cli, "--agent-skill-check", "off", "pack", "--tests", "--rust", "--rt", "--from", corpus, "--rust-profile", "fast", "--rust-workspace", workspace, "--rust-build-dir", scratch / "pack", "-o", binary]
    for attempt in range(len(vm_names) + 1):
        pack = invoke(pack_command, env=env, cwd=workspace, timeout=7200, evidence=scratch / f"pack-{attempt}")
        expired = pack.get("admission_timeout") or pack.get("admission_crash")
        if expired is None:
            if pack["code"] and not pack["timeout"]:
                failures = isolate_native_compilation_errors(pack["stderr"], pack_dir / "src/main.rs",
                                                            scratch / "admission", native_originals)
                if failures:
                    for entry, reason in failures:
                        print(f"NATIVE COMPILE CRASH entry {entry}: {reason}; restarting with other entries", flush=True)
                    continue
            break
        cache_file = Path(expired["cache_file"]).resolve()
        cache_file.relative_to(Path(env["BAML_RT_CORPUS_ADMISSION_CACHE"]).resolve())
        reason = (f"RT admission timed out after {timeout}s" if pack.get("admission_timeout")
                  else f"RT admission process crashed (exit {pack['code']})")
        write_json(cache_file, {"Err":reason})
        print(f"ADMISSION CRASH {expired['name']}: {reason}; restarting with completed admissions cached", flush=True)

    if pack["code"] or pack["timeout"]:
        raise RuntimeError(f"RT corpus pack failed: {pack['stderr'][-7000:]}")
    profile_exclusions = [json.loads(line.split("\t", 1)[1]) for line in pack["stderr"].splitlines()
                          if line.startswith("BAML_CORPUS_PROFILE_EXCLUDED\t")]
    for item in profile_exclusions:
        print(f"EXCLUDED PROFILE {item['kind']}: {item['name']}")
    listing = invoke([binary, "--list"], env=env, cwd=workspace, timeout=timeout, evidence=scratch / "rt-inventory")
    if listing["code"] or listing["timeout"]:
        raise RuntimeError(f"RT inventory failed: {listing}")
    inventory = json.loads(listing["stdout"])
    inventory = {row["name"]: row["admission"] for row in inventory}
    missing = set(vm_names) - inventory.keys()
    if missing:
        raise RuntimeError(f"RT inventory lost VM tests: {sorted(missing)}")
    report = {"schema": 1, "workspace": str(workspace), "vm_source_fingerprint": fingerprint,
              "profile_exclusions": profile_exclusions, "tests": {}, "vm_total": len(vm_names)}
    for index, name in enumerate(sorted(vm_names)):
        admission = inventory[name]
        vm_outcome = "pass" if name in vm["passed"] else "tolerated" if name in vm["tolerated"] else "fail"
        if admission is not None:
            category = admission_category(admission)
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
                signatures.append(f"{mode}: VM {oracle['outcome']} / RT {native['outcome']}; " + "; ".join(field for field in ("messages", "output", "stderr") if oracle[field] != native[field]))
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
    write_json(output, report, indent=2, sort_keys=True)
    print(f"Evidence: {output}")
    if args.select:
        if args.update_baseline: raise RuntimeError("a subset cannot update the full baseline")
        return 0 if all(row["category"] not in ("mismatch", "crash/timeout") for row in report["tests"].values()) else 1
    if args.update_baseline:
        baseline = {"schema": 1, "counts":report["counts"], "tests": {name: {key: row[key] for key in ("category", "signature", "fingerprint") if key in row} for name,row in report["tests"].items()}}
        write_json(args.baseline, baseline, indent=2, sort_keys=True)
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
