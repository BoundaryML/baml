"""BAML v1 Actions conventions. Run from any directory; never execute workflow code."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path, PurePosixPath
import re
import shlex
import sys

from tree_sitter import Language, Parser
import tree_sitter_bash
import yaml
from yaml.nodes import MappingNode, ScalarNode, SequenceNode

HERE = Path(__file__).resolve().parent
MISE = ".github/actions/setup-mise/action.yml"
RUST = ".github/actions/setup-rust/action.yml"
R2 = ".github/actions/setup-sccache/action.yml"
# These are action identities, not a list of files to lint. Unknown remote actions
# fail closed: an innocently named third-party action can still install tools.
APPROVED_ACTIONS = {
    "boundaryml/setup-baml": "Explicit approved BAML toolchain installer",
    "actions/checkout": "Source checkout",
    "useblacksmith/checkout": "Source checkout",
    "actions/cache": "Non-Rust dependency caches (paths checked separately)",
    "actions/cache/restore": "Non-Rust dependency cache restore",
    "actions/cache/save": "Non-Rust dependency cache save",
    "actions/download-artifact": "Artifact transfer",
    "actions/upload-artifact": "Artifact transfer",
    "actions/upload-artifact/merge": "Artifact transfer",
    "actions/upload-pages-artifact": "Pages artifact transfer",
    "actions/deploy-pages": "Pages deployment",
    "actions/github-script": "GitHub API scripting",
    "aws-actions/configure-aws-credentials": "AWS authentication",
    "infisical/secrets-action": "Secret retrieval",
    "nuget/login": "NuGet authentication",
    "rust-lang/crates-io-auth-action": "crates.io authentication",
    "pypa/gh-action-pypi-publish": "Upload built Python distributions",
    "peter-evans/create-pull-request": "PR creation",
    "marocchino/sticky-pull-request-comment": "PR reporting",
    "softprops/action-gh-release": "Release publication",
    "codspeedhq/action": "Benchmark execution",
    "sebrollen/toml-action": "Read TOML metadata",
    "pyo3/maturin-action": "Wheel build in ABI/manylinux containers; not a host tool setup action",
}
BOOTSTRAP_ACTIONS = {
    (MISE, "cargo-bins/cargo-binstall"),
    (MISE, "jdx/mise-action"),
}
EXPR = re.compile(r"\$\{\{.*?\}\}", re.S)
RUST_BUILD = {
    "build",
    "b",
    "check",
    "c",
    "test",
    "t",
    "run",
    "r",
    "doc",
    "rustc",
    "clippy",
    "nextest",
    "bench",
    "codspeed",
}


@dataclass(frozen=True, order=True)
class Diagnostic:
    path: str
    line: int
    column: int
    rule: str
    context: str
    message: str

    def __str__(self):
        return f"{self.path}:{self.line}:{self.column}: {self.rule}: {self.context}: {self.message}"


class InvalidYaml(ValueError):
    pass


def convert(node, ancestors=()):
    """Use YAML's representation graph, preserving `on` and source locations.

    Unlike safe_load's YAML 1.1 coercion, all scalar keys stay strings. Reject
    duplicates and recursive aliases rather than silently hiding a set of steps.
    """
    if id(node) in ancestors:
        raise InvalidYaml("recursive YAML alias")
    ancestors = (*ancestors, id(node))
    if isinstance(node, ScalarNode):
        return node.value
    if isinstance(node, SequenceNode):
        return [convert(v, ancestors) for v in node.value]
    if isinstance(node, MappingNode):
        result = {}
        for k, v in node.value:
            if not isinstance(k, ScalarNode):
                raise InvalidYaml("mapping keys must be scalar strings")
            if k.value in result or k.value == "<<":
                raise InvalidYaml(f"duplicate or unsupported merge key: {k.value}")
            result[k.value] = convert(v, ancestors)
        return result
    raise InvalidYaml("unsupported YAML node")


def children(node):
    if isinstance(node, MappingNode):
        return {k.value: v for k, v in node.value}
    return {}


def discover(root: Path):
    """All workflows and repository action metadata, including untracked files.

    Prune only VCS/dependency directories; no workflow/action name filters.
    """
    found = set()
    for directory, dirs, files in os.walk(root):
        dirs[:] = sorted(
            d for d in dirs if d not in {".git", ".jj", "node_modules", ".venv", "__pycache__"}
        )
        for filename in files:
            p = Path(directory, filename).relative_to(root)
            if filename in {"action.yml", "action.yaml"} or (
                p.parent == Path(".github/workflows") and p.suffix in {".yml", ".yaml"}
            ):
                found.add(p.as_posix())
    return sorted(found)


def shell_commands(source: str):
    """Read executable Bash commands, including substitutions and shell -c.

    Comments, echo arguments and literal heredocs are not executable commands.
    GitHub expressions are opaque words, retaining newlines for diagnostics.
    """
    source = EXPR.sub(lambda m: "BAML_EXPRESSION" + "\n" * m.group().count("\n"), source)
    parser = Parser(Language(tree_sitter_bash.language()))
    tree = parser.parse(source.encode())
    stack = [tree.root_node]
    while stack:
        node = stack.pop()
        stack.extend(reversed(node.named_children))
        if node.type != "command":
            continue
        name = node.child_by_field_name("name")
        if name is None:
            continue
        args = [name, *node.children_by_field_name("argument")]
        words = []
        for arg in args:
            raw = arg.text.decode()
            try:
                parts = shlex.split(raw)
                words.append(parts[0] if len(parts) == 1 else raw)
            except ValueError:
                words.append(raw)
        while words and words[0] in {"sudo", "env", "command", "exec", "timeout"}:
            words.pop(0)
            while words and (words[0].startswith("-") or "=" in words[0] or words[0].isdigit()):
                words.pop(0)
        if not words:
            continue
        words[0] = words[0].replace("\\", "/").rsplit("/", 1)[-1].removesuffix(".exe").lower()
        row = node.start_point.row
        yield words, row
        if words[0] in {"bash", "sh", "zsh"} and any(x in words for x in ("-c", "-lc", "-ec")):
            for flag in ("-c", "-lc", "-ec"):
                if flag in words and words.index(flag) + 1 < len(words):
                    for nested, offset in shell_commands(words[words.index(flag) + 1]):
                        yield nested, row + offset


def installation(words):
    cmd, *args = words
    args = [a for a in args if a not in {"--no-cache", "--quiet", "-q", "--verbose", "-v"}]
    # cargo tool installation != cargo build/test; uv tool install != uv sync/run.
    cargo_args = [a for a in args if not a.startswith(("-", "+"))]
    if cmd in {"cargo", "cargo-binstall"} and (
        cmd == "cargo-binstall" or cargo_args[:1] in (["install"], ["binstall"])
    ):
        return True
    if cmd == "go" and "install" in args:
        return True
    if (
        cmd in {"npm", "pnpm", "yarn"}
        and any(a in args for a in {"install", "i", "add"})
        and any(a in args for a in {"-g", "--global", "global"})
    ):
        return True
    if cmd == "corepack" and any(a in args for a in {"enable", "prepare", "install", "use"}):
        return True
    if cmd == "uv" and (args[:2] in (["tool", "install"], ["python", "install"])):
        return True
    if cmd == "pipx" and "install" in args:
        return True
    if cmd == "dotnet" and args[:2] == ["tool", "install"]:
        return True
    if cmd == "mise" and any(a in args for a in {"install", "i", "use", "upgrade"}):
        return True
    # pip installing project dependencies is allowed; replacing a pinned tool is not.
    if cmd in {"pip", "pip3", "python", "python3", "uv"} and "install" in args:
        return any(re.match(r"^(uv|ruff|maturin|poetry|pipx)([<=>\[]|$)", a) for a in args)
    if cmd in {"curl", "wget", "invoke-webrequest", "irm", "iwr"}:
        return any(
            re.search(
                r"(sh\.rustup\.rs|rustup-init|mise\.run|astral\.sh/uv|install\.(sh|ps1)|get\.pnpm|nodejs\.org/.*/.*\.(tar|zip))",
                a,
            )
            for a in args
        )
    return False


def rust_build(words):
    cmd, *args = words
    if cmd in {"$cargo", "${cargo}"}:
        cmd = "cargo"
    return (
        cmd in {"cargo", "cross"}
        and next((a for a in args if not a.startswith(("-", "+"))), "") in RUST_BUILD
    ) or (cmd == "wasm-pack" and args[:1] in (["build"], ["test"]))


class Linter:
    def __init__(self, root, exclusions=None):
        self.root = Path(root).resolve()
        self.exclusions = (
            exclusions
            if exclusions is not None
            else json.loads((HERE / "exclusions.json").read_text())
        )
        self.diagnostics = []
        self.documents = {}
        self.files = discover(self.root)

    def emit(self, path, node, rule, context, message, offset=0):
        mark = node.start_mark if node is not None else None
        self.diagnostics.append(
            Diagnostic(
                path,
                (mark.line + 1 if mark else 1) + offset,
                mark.column + 1 if mark else 1,
                rule,
                context,
                message,
            )
        )

    def run(self):
        for path, reason in self.exclusions.items():
            if (
                path not in self.files
                or any(c in path for c in "*?[]")
                or not isinstance(reason, str)
                or not reason.strip()
            ):
                self.emit(
                    path,
                    None,
                    "scope",
                    "exclusion",
                    "exclusions must name an existing exact file and include a reason; remove stale entries",
                )
        for path in self.files:
            if path in self.exclusions:
                continue
            try:
                node = yaml.compose((self.root / path).read_text(), Loader=yaml.SafeLoader)
                doc = convert(node)
                if not isinstance(doc, dict):
                    raise InvalidYaml("expected a mapping")
                self.documents[path] = (doc, node)
            except (yaml.YAMLError, InvalidYaml, UnicodeError, OSError) as e:
                mark = getattr(e, "problem_mark", None)
                self.diagnostics.append(
                    Diagnostic(
                        path,
                        mark.line + 1 if mark else 1,
                        mark.column + 1 if mark else 1,
                        "yaml",
                        "document",
                        str(e),
                    )
                )
        for path, (doc, node) in self.documents.items():
            self.check_environment(path, node)
            if path.startswith(".github/workflows/"):
                jobs = doc.get("jobs")
                if not isinstance(jobs, dict) or not jobs:
                    self.emit(
                        path, node, "structure", "workflow", "expected a nonempty jobs mapping"
                    )
                    continue
                jnodes = children(children(node).get("jobs"))
                for job, value in jobs.items():
                    if not isinstance(value, dict):
                        self.emit(path, jnodes[job], "structure", job, "expected a job mapping")
                    elif "uses" in value:
                        self.check_uses(path, jnodes[job], value, job, reusable=True)
                    else:
                        self.steps(path, value, jnodes[job], f"job {job}")
                        if isinstance(value.get("steps"), list) and any(
                            self.provides_r2(s) for s in value["steps"] if isinstance(s, dict)
                        ):
                            env = {
                                **(doc.get("env") if isinstance(doc.get("env"), dict) else {}),
                                **(value.get("env") if isinstance(value.get("env"), dict) else {}),
                            }
                            for key in (
                                "BAML_SCCACHE_R2_ACCESS_KEY_ID",
                                "BAML_SCCACHE_R2_SECRET_ACCESS_KEY",
                            ):
                                if env.get(key) != "${{ secrets." + key + " }}":
                                    self.emit(
                                        path,
                                        jnodes[job],
                                        "r2-credentials",
                                        f"job {job}",
                                        f"map optional secrets.{key} to env.{key}; absent secrets still use the .envrc local fallback",
                                    )
            else:
                runs = doc.get("runs")
                if not isinstance(runs, dict):
                    self.emit(path, node, "structure", "action", "expected runs mapping")
                elif runs.get("using") == "composite":
                    self.steps(path, runs, children(node).get("runs"), "composite")
                elif runs.get("using") not in {"node20", "node24", "docker"}:
                    self.emit(path, node, "structure", "action", "unknown action runtime")
                else:
                    self.emit(
                        path,
                        node,
                        "action-runtime",
                        "action",
                        "executable JavaScript/Docker actions require explicit policy review; use a composite action for inspectable tool setup",
                    )
        return sorted(set(self.diagnostics))

    def check_environment(self, path, node):
        if isinstance(node, MappingNode):
            mapping = children(node)
            for key, value in children(mapping.get("env")).items():
                if key in {
                    "SCCACHE_GHA_ENABLED",
                    "SCCACHE_BUCKET",
                    "SCCACHE_ENDPOINT",
                    "SCCACHE_REGION",
                    "SCCACHE_S3_KEY_PREFIX",
                    "SCCACHE_REDIS",
                }:
                    self.emit(
                        path,
                        value,
                        "cache-config",
                        "env",
                        f"{key} must come from the canonical .envrc R2 configuration",
                    )
                if key == "MISE_TASK_RUN_AUTO_INSTALL" and value.value != "false":
                    self.emit(
                        path,
                        value,
                        "mise-config",
                        "env",
                        "CI must not auto-install tools when running mise tasks",
                    )
                if key == "RUSTC_WRAPPER" and value.value != "baml-sccache":
                    self.emit(
                        path,
                        value,
                        "cache-config",
                        "env",
                        "RUSTC_WRAPPER must use the repository's credential-mapping baml-sccache wrapper",
                    )
            for child in mapping.values():
                self.check_environment(path, child)
        elif isinstance(node, SequenceNode):
            for child in node.value:
                self.check_environment(path, child)

    def local_path(self, uses, reusable=False):
        raw = uses[2:]
        # PurePosixPath normalizes ./ and repeated slashes; reject .. traversal.
        if ".." in PurePosixPath(raw).parts or "@" in raw:
            return None
        path = PurePosixPath(raw).as_posix()
        if reusable:
            return path
        for name in ("action.yml", "action.yaml"):
            candidate = f"{path}/{name}"
            if candidate in self.files:
                return candidate
        return None

    def check_uses(self, path, node, step, context, reusable=False):
        uses = step.get("uses")
        if not isinstance(uses, str):
            self.emit(path, node, "structure", context, "uses must be a string")
            return
        if uses.startswith("./"):
            target = self.local_path(uses, reusable)
            if target is None or target not in self.files:
                self.emit(
                    path, node, "local-action", context, f"cannot resolve local reference {uses}"
                )
            elif target in self.exclusions and not reusable:
                self.emit(
                    path,
                    node,
                    "v0-action",
                    context,
                    f"{uses} is a v0 action; use setup-mise or a checked v1 composite",
                )
            return
        identity, sep, ref = uses.partition("@")
        identity = identity.lower()
        if (
            not sep
            or not ref
            or (identity not in APPROVED_ACTIONS and (path, identity) not in BOOTSTRAP_ACTIONS)
        ):
            self.emit(
                path,
                node,
                "tool-action",
                context,
                f"unapproved action {uses}; install tools via ./.github/actions/setup-mise and use R2 sccache for Rust; approve non-setup actions explicitly in rules.py",
            )
        if identity.startswith("actions/cache"):
            config = step.get("with", {})
            if not isinstance(config, dict):
                return
            cache_path = config.get("path", "")
            if not isinstance(cache_path, str) or not cache_path.strip():
                self.emit(path, node, "structure", context, "cache path must be a nonempty string")
                return
            # Opaque paths cannot prove that the cache is non-Rust.
            for line in cache_path.replace("\\", "/").splitlines():
                normalized = EXPR.sub("EXPR", line)
                if (
                    re.search(
                        r"(^|/)\.?cargo(/|$)|(^|/)target(/|$)|sccache|CARGO_HOME|CARGO_TARGET_DIR",
                        line,
                        re.I,
                    )
                    or ("EXPR" in normalized and "/" not in normalized)
                    or line.strip() in {".", "./", "**", "**/*"}
                ):
                    self.emit(
                        path,
                        node,
                        "rust-cache",
                        context,
                        "Rust artifacts and registries must use the .envrc R2 sccache configuration, not actions/cache",
                    )
                    break

    def provides_r2(self, step, seen=()):
        if step.get("if") not in (None, "true", "${{ true }}"):
            return False
        uses = step.get("uses", "")
        if not isinstance(uses, str):
            return False
        if uses.startswith("./"):
            path = self.local_path(uses)
            if path in seen or path not in self.documents:
                return False
            doc, _ = self.documents[path]
            if not isinstance(doc.get("runs"), dict) or not isinstance(
                doc["runs"].get("steps"), list
            ):
                return False
            return any(
                self.provides_r2(s, (*seen, path))
                for s in doc.get("runs", {}).get("steps", [])
                if isinstance(s, dict)
            )
        if not isinstance(step.get("run", ""), str):
            return False
        commands = list(shell_commands(step.get("run", "")))
        return (
            any(w == ["direnv", "export", "gha"] for w, _ in commands)
            and any(w == ["direnv", "allow", ".envrc"] for w, _ in commands)
            and bool(re.search(r'>>\s*["\']?\$\{?GITHUB_ENV\}?', step.get("run", "")))
        )

    def provides_cache_tools(self, step, seen=()):
        if step.get("if") not in (None, "true", "${{ true }}"):
            return False
        uses = step.get("uses", "")
        if not isinstance(uses, str) or not uses.startswith("./"):
            return False
        path = self.local_path(uses)
        if path == MISE:
            config = step.get("with", {})
            args = config.get("install_args", "") if isinstance(config, dict) else ""
            return isinstance(args, str) and {"sccache", "direnv"} <= {
                x.split("@")[0] for x in args.split()
            }
        if path in seen or path not in self.documents:
            return False
        doc, _ = self.documents[path]
        runs = doc.get("runs", {})
        return (
            isinstance(runs, dict)
            and isinstance(runs.get("steps"), list)
            and any(
                self.provides_cache_tools(s, (*seen, path))
                for s in runs["steps"]
                if isinstance(s, dict)
            )
        )

    def steps(self, path, parent, node, context):
        steps = parent.get("steps")
        snode = children(node).get("steps")
        if not isinstance(steps, list) or not isinstance(snode, SequenceNode):
            self.emit(path, node, "structure", context, "expected a steps sequence")
            return
        r2 = False
        cache_tools = False
        for i, (step, item) in enumerate(zip(steps, snode.value), 1):
            ctx = (
                f"{context}, step {i} ({step.get('name', step.get('id', 'unnamed'))})"
                if isinstance(step, dict)
                else context
            )
            if not isinstance(step, dict):
                self.emit(path, item, "structure", ctx, "expected a step mapping")
                continue
            if "uses" in step:
                self.check_uses(path, item, step, ctx)
                if isinstance(step["uses"], str) and step["uses"].lower().startswith(
                    "pyo3/maturin-action@"
                ):
                    config = step.get("with", {})
                    if (
                        not isinstance(config, dict)
                        or config.get("command", "build") != "build"
                        or config.get("sccache", "false") != "false"
                    ):
                        self.emit(
                            path,
                            item,
                            "wheel-build",
                            ctx,
                            "maturin-action is approved only for wheel builds, with its alternative GitHub cache disabled",
                        )
                    if not r2:
                        self.emit(
                            path,
                            item,
                            "r2-required",
                            ctx,
                            "wheel builds require earlier canonical R2 setup",
                        )
            if "run" in step:
                run = step["run"]
                rnode = children(item)["run"]
                if not isinstance(run, str):
                    self.emit(path, rnode, "structure", ctx, "run must be a string")
                    continue
                if self.provides_r2(step) and not cache_tools:
                    self.emit(
                        path,
                        rnode,
                        "r2-tools",
                        ctx,
                        "install sccache and direnv through setup-mise before loading .envrc",
                    )
                sources = [(run, rnode)]
                # Matrix-provided shell snippets are executable too. Resolve the
                # referenced static axis/include fields, not arbitrary strings
                # in the document (names and descriptions are not commands).
                for match in re.finditer(r"\$\{\{\s*matrix\.([\w.]+)\s*\}\}", run):
                    matrix_node = children(children(node).get("strategy")).get("matrix")

                    def values(current, fields):
                        if isinstance(current, SequenceNode):
                            for entry in current.value:
                                yield from values(entry, fields)
                        elif not fields:
                            if isinstance(current, ScalarNode):
                                yield current
                        elif isinstance(current, MappingNode):
                            mapping = children(current)
                            if fields[0] in mapping:
                                yield from values(mapping[fields[0]], fields[1:])
                            if "include" in mapping:
                                yield from values(mapping["include"], fields)

                    for value in values(matrix_node, match[1].split(".")):
                        sources.append((run.replace(match[0], value.value), value))
                if EXPR.fullmatch(run.strip()) and len(sources) == 1:
                    self.emit(
                        path,
                        rnode,
                        "dynamic-run",
                        ctx,
                        "cannot resolve executable expression; put shell commands directly in run or in a static matrix field",
                    )
                for source, source_node in sources:
                    self.check_commands(path, source_node, ctx, source, r2)
            r2 = r2 or self.provides_r2(step)
            cache_tools = cache_tools or self.provides_cache_tools(step)

    def check_commands(self, path, rnode, ctx, run, r2):
        for words, row in shell_commands(run):
            # Rustup itself is the canonical Rust manager. Only its
            # download/bootstrap belongs inside setup-rust.
            if installation(words) and not (
                path == RUST
                and words[0] in {"curl", "invoke-webrequest"}
                and any("rustup" in w for w in words)
            ):
                self.emit(
                    path,
                    rnode,
                    "tool-install",
                    ctx,
                    f"tool installation ({' '.join(words[:4])}) must move to setup-mise",
                    row + (1 if rnode.style in {"|", ">"} else 0),
                )
            if rust_build(words) and not r2:
                self.emit(
                    path,
                    rnode,
                    "r2-required",
                    ctx,
                    "Rust compilation requires an earlier unconditional setup-sccache (or existing direnv allow/export gha) step",
                    row + (1 if rnode.style in {"|", ">"} else 0),
                )


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=HERE.parents[1])
    parser.add_argument("--format", choices=["text", "json", "github"], default="text")
    parser.add_argument("--list-exclusions", action="store_true")
    args = parser.parse_args(argv)
    lint = Linter(args.root)
    if args.list_exclusions:
        for path, reason in sorted(lint.exclusions.items()):
            print(f"{path}: {reason}")
        return 0
    diagnostics = lint.run()
    if args.format == "json":
        from dataclasses import asdict

        print(json.dumps([asdict(d) for d in diagnostics], indent=2))
    else:
        for d in diagnostics:
            if args.format == "github":

                def escape(s):
                    return (
                        str(s)
                        .replace("%", "%25")
                        .replace("\r", "%0D")
                        .replace("\n", "%0A")
                        .replace(",", "%2C")
                        .replace(":", "%3A")
                    )

                print(
                    f"::error file={escape(d.path)},line={d.line},col={d.column},title={escape(d.rule)}::{escape(d.context + ': ' + d.message)}"
                )
            else:
                print(d)
        print(
            f"Checked {len(lint.documents)} files; excluded {len(lint.exclusions)} exact v0 paths; {len(diagnostics)} violation(s)."
        )
    return bool(diagnostics)


if __name__ == "__main__":
    sys.exit(main())
