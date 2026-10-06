"""Git snapshot adapter. Policy execution and its typed result stay in BAML."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parent
POLICY_PATH = ".github/codeturtle"
HUNK = re.compile(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@")




class InputError(Exception):
    pass


def git(repo, *arguments):
    result = subprocess.run(
        ["git", "-C", str(repo), "--literal-pathspecs", *arguments],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=60,
    )
    if result.returncode:
        raise InputError(result.stderr.decode("utf-8", "replace").strip())
    return result.stdout


def revision(repo, name):
    return git(repo, "rev-parse", "--verify", "--end-of-options", name + "^{commit}").decode().strip()


def decode_path(data):
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError as error:
        raise InputError("Non-UTF-8 Git paths are not supported yet") from error


def blob(repo, commit, path):
    return git(repo, "cat-file", "blob", commit + ":" + path)


def text_content(data):
    if data is None:
        return None
    if b"\0" in data:
        return None
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        return None


def parse_hunks(patch):
    hunks = []
    for line in patch.splitlines():
        header = HUNK.match(line)
        if header:
            old_start, old_count, new_start, new_count = header.groups()
            hunks.append({
                "old_start": int(old_start), "old_count": int(old_count or "1"),
                "new_start": int(new_start), "new_count": int(new_count or "1"),
                "added_lines": [], "removed_lines": [],
            })
        elif hunks and line.startswith("+"):
            hunks[-1]["added_lines"].append(line[1:])
        elif hunks and line.startswith("-"):
            hunks[-1]["removed_lines"].append(line[1:])
    return hunks


def read_change(repo, merge_base, head, entry):
    status, path, old_path = entry
    before_bytes = None if status == "A" else blob(repo, merge_base, old_path or path)
    after_bytes = None if status == "D" else blob(repo, head, path)
    before, after = text_content(before_bytes), text_content(after_bytes)
    binary = (before_bytes is not None and before is None) or (after_bytes is not None and after is None)
    patch = git(
        repo, "diff", "--no-ext-diff", "--no-textconv", "--no-color", "--find-renames",
        "--unified=0", *([] if binary else ["--text"]), merge_base, head, "--",
        *(list(dict.fromkeys([old_path, path])) if old_path else [path]),
    ).decode("utf-8", "replace")
    hunks = [] if binary else parse_hunks(patch)
    return {
        "path": path, "old_path": old_path, "status": status,
        "before": before, "after": after, "is_binary": binary, "diff": patch, "hunks": hunks,
        "added_code": "\n".join(line for hunk in hunks for line in hunk["added_lines"]),
        "removed_code": "\n".join(line for hunk in hunks for line in hunk["removed_lines"]),
    }


def changes(repo, merge_base, head, concurrency=8):
    fields = git(repo, "diff", "--no-ext-diff", "--no-textconv", "--name-status", "-z", "--find-renames", merge_base, head, "--").split(b"\0")
    entries = []
    index = 0
    while index < len(fields) and fields[index]:
        status = fields[index].decode("ascii")
        index += 1
        first = decode_path(fields[index])
        index += 1
        old_path = None
        if status.startswith("R"):
            old_path = first
            path = decode_path(fields[index])
            index += 1
        else:
            path = first
        entries.append((status, path, old_path))
    with ThreadPoolExecutor(max_workers=concurrency) as pool:
        # Executor.map preserves input order even when reads finish out of order.
        return list(pool.map(lambda entry: read_change(repo, merge_base, head, entry), entries))


def write_snapshot(repo, base, destination, concurrency=8):
    target = destination / POLICY_PATH
    target.mkdir(parents=True)
    entries = git(repo, "ls-tree", "-r", "-z", "--full-tree", base, "--", POLICY_PATH).split(b"\0")
    files = []
    for entry in entries:
        if not entry:
            continue
        metadata, raw_path = entry.split(b"\t", 1)
        mode, kind, object_id = metadata.split()
        path = PurePosixPath(decode_path(raw_path))
        if path.name != "OWNERS" and path.suffix != ".baml":
            continue
        if path.is_absolute() or ".." in path.parts or path.parts[:2] != (".github", "codeturtle"):
            raise InputError("Invalid policy path in Git snapshot")
        if kind != b"blob" or mode not in (b"100644", b"100755"):
            raise InputError("Policy files must be regular files: " + str(path))
        files.append((path, object_id.decode()))
    with ThreadPoolExecutor(max_workers=concurrency) as pool:
        contents = pool.map(lambda file: git(repo, "cat-file", "blob", file[1]), files)
        for (path, _), content in zip(files, contents):
            local = destination.joinpath(*path.parts)
            local.parent.mkdir(parents=True, exist_ok=True)
            local.write_bytes(content)
    if not (target / "OWNERS").is_file():
        raise InputError(
            "No .github/codeturtle/OWNERS at base revision " + base
            + ". For a local preview, explicitly pass --policy-dir <directory>."
        )


def write_local_policy(source, destination):
    source = source.resolve()
    target = destination / POLICY_PATH
    target.mkdir(parents=True)
    for path in sorted(source.rglob("*")):
        if path.is_symlink():
            raise InputError("Local policy symlinks are not supported: " + str(path))
        if path.is_file() and (path.name == "OWNERS" or path.suffix == ".baml"):
            local = target / path.relative_to(source)
            local.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, local)
    if not (target / "OWNERS").is_file():
        raise InputError("The local policy directory must contain OWNERS")


def run_baml(arguments, target=None):
    result = subprocess.run(
        ["baml", "run", "--project", str(ROOT), "--output-format", "json",
         target or "main", "--", *arguments],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=120,
    )
    if result.stderr:
        sys.stderr.write(result.stderr)
    if result.returncode:
        raise InputError("BAML execution failed (exit " + str(result.returncode) + ")")
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise InputError("BAML did not return a JSON report") from error


def evaluate(repo, changes_path, concurrency=8):
    return run_baml(["--repo", str(repo), "--changes_path", str(changes_path), "--concurrency", str(concurrency)])


def reflected_help():
    report = run_baml([], target="agent_help")
    # Reflection renders local type names. Qualify them using the discovered
    # declarations, so agents can paste types into a separately compiled policy.
    names = {item["name"].removeprefix("codeturtle."): item["name"] for item in report["types"]}
    pattern = re.compile(r"(?<![\w.])(" + "|".join(re.escape(name) for name in names) + r")\b")

    def qualify(value):
        return pattern.sub(lambda match: names[match[1]], value.replace("root.", "codeturtle."))

    for item in report["types"]:
        for member in item["members"]:
            if member["type"]:
                member["type"] = qualify(member["type"])
    for contract in report["contracts"]:
        contract["input"] = qualify(contract["input"])
        contract["returns"] = qualify(contract["returns"])
    return report


def text_help(report):
    lines = ["CODETURTLE", "", report["guidance"].rstrip(), "", "Function inputs -> results"]
    for contract in report["contracts"]:
        lines.append(f"  ({contract['input']}) -> ({contract['returns']})")
        lines.append("    " + contract["invocation"])
    lines.extend(["", "Available types (reflected from BAML)"])
    for item in report["types"]:
        if item["kind"] == "enum":
            members = [member["name"] for member in item["members"]]
        else:
            members = [f"{member['name']}: {member['type']}" for member in item["members"]]
        lines.append("")
        lines.append(f"{item['kind']} {item['name']} {{")
        row = "  "
        for member in members:
            separator = ", " if row != "  " else ""
            if len(row + separator + member) > 92 and row != "  ":
                lines.append(row + ",")
                row, separator = "  ", ""
            row += separator + member
        lines.append(row)
        lines.append("}")
    return "\n".join(lines)


def error_summary(message):
    # Preserve the full compiler diagnostic in JSON; show its headline in text.
    match = re.search(r'CompilationError \{ message: ("(?:\\.|[^"\\])*")', message)
    if not match:
        return message
    try:
        summary = message[:match.start()] + json.loads(match[1])
        span = re.search(r'Span \{ file: ("(?:\\.|[^"\\])*"), start: (\d+), end: (\d+)', message)
        if span:
            summary += f" ({json.loads(span[1])}, offsets {span[2]}-{span[3]})"
        return summary
    except (ValueError, TypeError):
        return message


def text_report(report):
    state = "complete" if report["evaluation_complete"] else "incomplete"
    lines = [f"CodeTurtle: policy evaluation {state}"]
    context = report.get("context")
    if context:
        lines.append(f"Diff: {context['merge_base'][:12]}..{context['head_revision'][:12]}")
        if context["policy_source"] == "local_preview":
            lines.append("Policy: local preview")
        else:
            lines.append(f"Policy: base revision {context['policy_revision'][:12]}")
    grouped = {}
    for requirement in report["requirements"]:
        grouped.setdefault(requirement["path"], []).append(requirement)
    lines.append(f"{len(report['requirements'])} review requirements across {len(grouped)} files; {len(report['evaluations'])} evaluations")

    def block(text, indent):
        # Keep terminal control characters visible instead of executing them.
        for line in str(text).split("\n"):
            safe = "".join(character if ord(character) >= 32 and ord(character) != 127 else f"\\x{ord(character):02x}" for character in line)
            lines.append(indent + safe)

    for path, requirements in grouped.items():
        lines.append("")
        block(path, "")
        for requirement in requirements:
            evaluation = requirement["evaluation"]
            result = evaluation["result"]
            threshold = "all of" if requirement["approval"] == "All" else "any of"
            source = f"OWNERS:{evaluation['rule_line']}"
            if evaluation["condition"]:
                source += " / " + evaluation["condition"]
            block(f"{threshold} {', '.join(requirement['owners'])} [{source}]", "  ")
            reason = result["reason"]
            if result["verdict"] == "Uncertain":
                reason = "Uncertain: " + reason
            block(reason, "    ")
            for evidence in result["evidence"]:
                block("Evidence: " + evidence, "    ")
    if report["unowned_files"]:
        lines.extend(["", f"Unowned files ({len(report['unowned_files'])}):"])
        for path in report["unowned_files"]:
            block(path, "  ")
    if report["errors"]:
        lines.extend(["", f"Errors ({len(report['errors'])}):"])
        for error in report["errors"]:
            source = f"OWNERS:{error['rule_line']}: " if error["rule_line"] is not None else ""
            block(source + error_summary(error["message"]), "  ")
    return "\n".join(lines)


def emit(report, mode):
    if mode == "json":
        print(json.dumps(report, indent=2))
    elif "valid" in report:
        state = "valid" if report["valid"] else "invalid"
        lines = [f"CodeTurtle: policy {state}",
                 f"{report['rule_count']} ownership rules; {report['source_count']} BAML files; {len(report['conditions'])} referenced conditions"]
        for condition in report["conditions"]:
            lines.append(f"  {condition['name']} ({condition['scope']})")
        if report["errors"]:
            lines.extend(["", f"Errors ({len(report['errors'])}):"])
            for error in report["errors"]:
                source = f"OWNERS:{error['rule_line']}: " if error["rule_line"] is not None else ""
                lines.append("  " + source + error_summary(error["message"]))
        print("\n".join(lines))
    else:
        print(text_report(report))


def add_common_options(parser, defaults=False):
    parser.add_argument("--repo", type=Path, default=Path.cwd() if defaults else argparse.SUPPRESS,
                        help="Repository directory (default: current directory)")
    parser.add_argument("--mode", choices=("json", "text"), default="text" if defaults else argparse.SUPPRESS,
                        help="Output format (default: text)")
    parser.add_argument("--concurrency", type=int, default=8 if defaults else argparse.SUPPRESS,
                        help="Maximum parallel Git reads and policy calls (default: 8; use 1 for sequential)")


def parse_arguments(arguments):
    parser = argparse.ArgumentParser(prog="codeturtle", description="Ownership rules and BAML conditions for code review.",
                                     epilog="For reflected BAML types and extension-writing guidance, run: codeturtle help")
    add_common_options(parser, defaults=True)
    subcommands = parser.add_subparsers(dest="command", metavar="COMMAND")
    check = subcommands.add_parser("check", aliases=["explain"], help="Evaluate review requirements for Git changes")
    add_common_options(check)
    input_mode = check.add_mutually_exclusive_group(required=True)
    input_mode.add_argument("--base", help="Target branch or commit; also supplies the trusted policy snapshot")
    input_mode.add_argument("--changes-path", "--changes_path", dest="changes_path", type=Path,
                            help="Evaluate a supplied JSON fixture instead")
    check.add_argument("--head", default="HEAD")
    check.add_argument("--title", help="PR title; defaults to the head commit subject")
    check.add_argument("--policy-dir", type=Path, help="Explicitly use local policy for development")
    lint = subcommands.add_parser("lint", help="Validate ownership syntax and BAML contracts without evaluating conditions")
    add_common_options(lint)
    policy_mode = lint.add_mutually_exclusive_group()
    policy_mode.add_argument("--policy-dir", type=Path, help="Local directory containing OWNERS and BAML files")
    policy_mode.add_argument("--rev", help="Validate committed policy at this Git revision instead of local policy")
    help_command = subcommands.add_parser("help", help="Show all reflected BAML types, contracts and authoring guidance")
    help_command.add_argument("--mode", choices=("json", "text"), default=argparse.SUPPRESS,
                              help="Reflected reference output format (default: text)")

    # Preserve existing flags-only calls as check. Skip global options and their
    # values first so e.g. a repo named 'lint' is never treated as a command.
    index = 0
    common = {"--repo", "--mode", "--concurrency"}
    while index < len(arguments) and arguments[index].split("=", 1)[0] in common:
        index += 1 if "=" in arguments[index] else 2
    check_options = {"--base", "--head", "--changes-path", "--changes_path", "--title", "--policy-dir"}
    if index < len(arguments) and arguments[index].split("=", 1)[0] in check_options:
        arguments = ["check", *arguments]
    args = parser.parse_args(arguments)
    if args.command is None:
        parser.print_help()
        return None
    return args


def lint(args):
    repo = args.repo.resolve()
    with tempfile.TemporaryDirectory(prefix="codeturtle-lint-") as directory:
        snapshot = Path(directory)
        if args.rev:
            repo = Path(git(repo, "rev-parse", "--show-toplevel").decode().strip())
            write_snapshot(repo, revision(repo, args.rev), snapshot, args.concurrency)
        else:
            if args.policy_dir:
                source = args.policy_dir
            else:
                source = repo / POLICY_PATH
                if not source.is_dir():
                    repo = Path(git(repo, "rev-parse", "--show-toplevel").decode().strip())
                    source = repo / POLICY_PATH
            write_local_policy(source, snapshot)
        return run_baml(["--repo", str(snapshot)], target="lint_policy")


def main():
    args = parse_arguments(sys.argv[1:])
    if args is None:
        return 0
    context = None
    try:
        if args.command == "help":
            report = reflected_help()
            if args.mode == "json":
                print(json.dumps(report, indent=2))
            else:
                print(text_help(report))
            return 0
        if args.concurrency < 1:
            raise InputError("--concurrency must be a positive integer")
        if args.command == "lint":
            report = lint(args)
            emit(report, args.mode)
            return 0 if report["valid"] else 1
        repo = args.repo.resolve()
        if args.changes_path:
            if args.policy_dir:
                raise InputError("--policy-dir is only supported with --base")
            report = evaluate(repo, args.changes_path.resolve(), args.concurrency)
        else:
            repo = Path(git(repo, "rev-parse", "--show-toplevel").decode().strip())
            base, head = revision(repo, args.base), revision(repo, args.head)
            merge_base = git(repo, "merge-base", base, head).decode().strip()
            context = {
                "base_revision": base, "head_revision": head, "merge_base": merge_base,
                "policy_revision": None if args.policy_dir else base,
                "policy_source": "local_preview" if args.policy_dir else "base_revision",
            }
            data = {
                "title": args.title if args.title is not None else git(repo, "show", "-s", "--format=%s", head).decode().strip(),
                "changes": changes(repo, merge_base, head, args.concurrency), "context": context,
            }
            with tempfile.TemporaryDirectory(prefix="codeturtle-git-") as directory:
                snapshot = Path(directory)
                if args.policy_dir:
                    write_local_policy(args.policy_dir, snapshot)
                else:
                    write_snapshot(repo, base, snapshot, args.concurrency)
                input_path = snapshot / "changes.json"
                input_path.write_text(json.dumps(data))
                report = evaluate(snapshot, input_path, args.concurrency)
                report["context"] = context
        emit(report, args.mode)
        return 0 if report["evaluation_complete"] else 1
    except (InputError, OSError, subprocess.TimeoutExpired) as error:
        if args.command == "help":
            if args.mode == "json":
                print(json.dumps({"errors": [{"message": str(error)}]}, indent=2))
            else:
                print("CodeTurtle: help unavailable\n" + str(error) + "\nUse codeturtle --help for command usage without BAML.")
            return 2
        report = {
            "context": context, "requirements": [], "evaluations": [], "unowned_files": [],
            "evaluation_complete": False, "errors": [{"rule_line": None, "message": str(error)}],
        }
        if args.command == "lint":
            report = {"valid": False, "rule_count": 0, "source_count": 0, "conditions": [], "errors": report["errors"]}
        emit(report, args.mode)
        return 2


if __name__ == "__main__":
    sys.exit(main())
