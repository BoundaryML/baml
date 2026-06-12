"""Actually run the code a changelog entry shows the reader (ported from baml-changelog2).

The reviewer (critique) step calls `check_body()` to exercise every fenced code
block in a draft entry's markdown body before the entry can be approved. The
results are authoritative: the critique LLM cannot run code, so the harness runs
it instead and feeds the verdict in.

What "run" means per language (chosen for low false-positive rate on the
intentionally-partial snippets a changelog shows):

  baml     `baml-cli fmt` -- parse-level validation. Catches syntax errors
           without needing a generator block, and WITHOUT the false
           "unresolved type" failures that full typechecking raises on a
           snippet that legitimately references types defined elsewhere.
  python   `compile()` -- syntax check only.
  json     `json.loads()` -- parse check.
  other    skipped (no safe, context-free checker).

Nothing here executes untrusted code with side effects: `baml-cli fmt` only
parses, python uses `compile()` (never `exec`), json uses `json.loads`. So it is
safe to run LLM-generated snippets server-side.
"""

from __future__ import annotations

import json
import logging
import os
import re
import shutil
import subprocess
import tempfile
from dataclasses import dataclass, field

log = logging.getLogger("changelog.runner")

# Path (or bare name) of the BAML CLI. Overridable for deploys where it lands
# somewhere other than $PATH.
BAML_CLI = os.environ.get("BAML_CLI", "baml-cli")
# Per-snippet wall-clock cap for the baml-cli subprocess.
BAML_TIMEOUT_S = int(os.environ.get("CHANGELOG_BAML_TIMEOUT_S", "20"))

# Languages we actively check. Everything else is recorded as skipped.
_LANG_ALIASES = {
    "py": "python",
    "ts": "typescript",
    "sh": "bash",
    "shell": "bash",
    "jsonc": "json",
}

_ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")
_OPEN_FENCE_RE = re.compile(r"^[ \t]*`{3,}\s*([\w+-]*)\s*$")
_CLOSE_FENCE_RE = re.compile(r"^[ \t]*`{3,}\s*$")


@dataclass
class BlockResult:
    index: int          # 1-based position among the body's code blocks
    lang: str           # normalized language tag
    status: str         # "pass" | "fail" | "skipped"
    error: str = ""     # compiler / parser message on fail; reason on skip
    code: str = ""      # the snippet itself


@dataclass
class CodeCheckReport:
    results: list[BlockResult] = field(default_factory=list)

    @property
    def failed(self) -> list[BlockResult]:
        return [r for r in self.results if r.status == "fail"]

    @property
    def checked(self) -> list[BlockResult]:
        return [r for r in self.results if r.status in ("pass", "fail")]

    def runnable_score(self) -> str:
        """The authoritative `runnable` critique dimension.

        Any failing block -> `fail`. Otherwise `great`: no code blocks, or every
        checkable block passed (skipped blocks don't block approval). `great`
        rather than a neutral score so that a perfectly fine entry with no code
        is not held back from approval, which requires every dimension good or
        great.
        """
        return "fail" if self.failed else "great"

    def summary_for_prompt(self) -> str:
        """Authoritative code-check results, injected into the critique prompt."""
        if not self.results:
            return "CODE CHECK: the body contains no fenced code blocks."
        lines = [
            "CODE CHECK (authoritative -- the harness actually ran/validated "
            "each block; trust this over your own reading of the code):"
        ]
        for r in self.results:
            if r.status == "pass":
                lines.append(f"- block #{r.index} ({r.lang}): PASS")
            elif r.status == "skipped":
                reason = r.error or "no checker for this language"
                lines.append(f"- block #{r.index} ({r.lang}): SKIPPED ({reason})")
            else:
                lines.append(f"- block #{r.index} ({r.lang}): FAIL")
                for ln in r.error.strip().splitlines():
                    lines.append(f"    {ln}")
        return "\n".join(lines)

    def issues(self) -> list[str]:
        """Short issue strings, merged into the critique's `issues` list."""
        out = []
        for r in self.failed:
            first = r.error.strip().splitlines()[0] if r.error.strip() else "did not validate"
            out.append(f"Code block #{r.index} ({r.lang}) does not run: {first}")
        return out

    def rewrite_hint(self) -> str:
        """Concrete redraft guidance, fed to the drafter alongside the critique."""
        lines = [
            "One or more code blocks in the body failed to run/validate. Fix "
            "or remove them:"
        ]
        for r in self.failed:
            lines.append(f"- block #{r.index} ({r.lang}):")
            for ln in r.error.strip()[:800].splitlines():
                lines.append(f"    {ln}")
        lines.append(
            "Every code example must be valid, self-consistent code grounded "
            "in the diff. If you cannot write a correct example, omit the code "
            "block rather than ship one that does not run."
        )
        return "\n".join(lines)


def _strip_ansi(s: str) -> str:
    return _ANSI_RE.sub("", s)


def _normalize_lang(tag: str) -> str:
    tag = tag.strip().lower()
    return _LANG_ALIASES.get(tag, tag)


def extract_blocks(body: str) -> list[BlockResult]:
    """Pull fenced code blocks out of the markdown body, in order.

    Line-based rather than regex so that BAML's `#"..."#` prompt strings and
    other backtick-free nesting inside a block are handled correctly. Returns
    BlockResults with status left unset (filled in by the checkers).
    """
    blocks: list[BlockResult] = []
    lines = body.splitlines()
    i, idx = 0, 0
    while i < len(lines):
        m = _OPEN_FENCE_RE.match(lines[i])
        if not m:
            i += 1
            continue
        j = i + 1
        buf: list[str] = []
        while j < len(lines) and not _CLOSE_FENCE_RE.match(lines[j]):
            buf.append(lines[j])
            j += 1
        idx += 1
        blocks.append(
            BlockResult(
                index=idx,
                lang=_normalize_lang(m.group(1)) or "text",
                status="",
                code="\n".join(buf),
            )
        )
        i = j + 1  # skip past the closing fence
    return blocks


# --- BAML: actually RUN the snippet, don't just parse it -----------------------
#
# Expression snippets are executed (`baml-cli run -e`); declaration snippets are
# typechecked (`baml-cli run --list`). A failure is then classified:
#   * llm      -> the snippet reaches a model/provider; we run with no API keys,
#                 so this is "skipped (would call an LLM)", never a failure.
#   * fragment -> incomplete / references context defined elsewhere (unresolved
#                 type, unknown identifier, statements with no return). We fall
#                 back to a parse check: pass if it at least parses, fail only if
#                 genuinely malformed. ("Run what's runnable, parse the rest.")
#   * bug      -> a real error on resolved code (e.g. `int[]` has no member
#                 `frobnicate`). Hard fail -> blocks approval, feeds the redraft.

_FRAGMENT_MARKERS = (
    "unresolved type", "unresolved symbol", "unknown identifier",
    "missing return value", "cannot find", "not found in scope",
    "is not defined", "undefined variable",
)
_LLM_MARKERS = (
    "api key", "api_key", "no api key", "environment variable", "unauthorized",
    " 401", "provider", "connection", "could not reach", "dns",
    "timed out", "timeout", "no client",
)
_DECL_RE = re.compile(
    r"(?m)^\s*(class|enum|function|test|client|generator|type|"
    r"template_string|retry_policy|dynamic)\b"
)


def _baml_shape(code: str) -> str:
    """'declaration' (top-level decls -> typecheck) vs 'expr' (run)."""
    return "declaration" if _DECL_RE.search(code) else "expr"


def _classify_baml_failure(err: str) -> str:
    e = err.lower()
    if any(m in e for m in _LLM_MARKERS):
        return "llm"
    if any(m in e for m in _FRAGMENT_MARKERS):
        return "fragment"
    return "bug"


def _baml_env() -> dict:
    """Env with model API keys stripped, so a snippet that calls an LLM fails
    fast (then classified as skipped) rather than spending money or hanging."""
    env = dict(os.environ)
    for k in list(env):
        u = k.upper()
        if any(s in u for s in ("ANTHROPIC", "OPENAI", "GOOGLE", "GEMINI",
                                "AZURE", "MISTRAL", "COHERE", "GROQ", "VERTEX",
                                "AWS_ACCESS", "AWS_SECRET")):
            env.pop(k, None)
    return env


def _run_baml(binary: str, args: list[str], cwd: str | None = None) -> tuple[int, str]:
    try:
        proc = subprocess.run(
            [binary, *args], capture_output=True, text=True,
            timeout=BAML_TIMEOUT_S, cwd=cwd, env=_baml_env(),
        )
    except subprocess.TimeoutExpired:
        return 124, f"timed out after {BAML_TIMEOUT_S}s"
    except OSError as e:
        return -1, f"could not run baml-cli: {e}"
    out = _strip_ansi((proc.stdout or "") + (proc.stderr or ""))
    out = "\n".join(l for l in out.splitlines() if "internal BAML toolchain" not in l)
    return proc.returncode, out.strip()


def _baml_parse_check(binary: str, code: str) -> tuple[str, str]:
    """`baml-cli fmt` -- the parse-level fallback for un-runnable fragments."""
    with tempfile.TemporaryDirectory() as d:
        path = os.path.join(d, "snippet.baml")
        with open(path, "w") as f:
            f.write(code if code.endswith("\n") else code + "\n")
        rc, err = _run_baml(binary, ["fmt", path])
    if rc == 0:
        return "pass", ""
    return "fail", (err.replace(path, "snippet.baml")[:1500] or f"fmt exited {rc}")


def _check_baml(code: str, binary: str | None) -> tuple[str, str]:
    if not binary:
        binary = shutil.which(BAML_CLI) or (BAML_CLI if os.path.exists(BAML_CLI) else None)
    if not binary:
        log.warning("baml-cli (%s) not found; skipping BAML validation", BAML_CLI)
        return "skipped", "baml-cli not found"

    if _baml_shape(code) == "declaration":
        with tempfile.TemporaryDirectory() as d:
            src = os.path.join(d, "baml_src")
            os.makedirs(src, exist_ok=True)
            with open(os.path.join(src, "snippet.baml"), "w") as f:
                f.write(code if code.endswith("\n") else code + "\n")
            rc, err = _run_baml(binary, ["run", "--list", "--from", d])
            err = err.replace(src, "baml_src").replace(d, ".")
    else:
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "expr.baml")
            with open(path, "w") as f:
                f.write(code if code.endswith("\n") else code + "\n")
            rc, err = _run_baml(binary, ["run", "-e", f"@{path}"])
            err = err.replace(path, "snippet.baml").replace(d, ".")

    if rc == 0:
        return "pass", ""
    cls = _classify_baml_failure(err)
    if cls == "llm":
        return "skipped", "snippet would call an LLM; not executed"
    if cls == "fragment":
        # Not runnable standalone (references external context / no return).
        # Parse-fallback: pass if it parses, fail only if genuinely malformed.
        return _baml_parse_check(binary, code)
    return "fail", err[:1500]


def _check_python(code: str) -> tuple[str, str]:
    try:
        compile(code, "<changelog-snippet>", "exec")
    except SyntaxError as e:
        return "fail", f"SyntaxError: {e.msg} (line {e.lineno})"
    return "pass", ""


def _check_json(code: str) -> tuple[str, str]:
    try:
        json.loads(code)
    except json.JSONDecodeError as e:
        return "fail", f"JSONDecodeError: {e.msg} (line {e.lineno}, col {e.colno})"
    return "pass", ""


# Non-BAML checkers take only the code; BAML is routed separately so it can use
# the per-entry pinned CLI.
_CHECKERS = {
    "python": _check_python,
    "json": _check_json,
}


def check_body(body: str, baml_cli: str | None = None) -> CodeCheckReport:
    """Extract and run every code block in `body`. Never raises.

    `baml_cli` is the path to the release-matched baml-cli for this entry (from
    `app.baml_cli.resolve_cli`); None falls back to the image's `baml-cli`."""
    report = CodeCheckReport()
    for block in extract_blocks(body):
        try:
            if block.lang == "baml":
                block.status, block.error = _check_baml(block.code, baml_cli)
            elif block.lang in _CHECKERS:
                block.status, block.error = _CHECKERS[block.lang](block.code)
            else:
                block.status, block.error = "skipped", f"no checker for `{block.lang}`"
        except Exception as e:  # never let a checker crash the request
            log.exception("code check crashed for block #%d (%s)", block.index, block.lang)
            block.status, block.error = "skipped", f"checker error: {e}"
        report.results.append(block)
    return report
