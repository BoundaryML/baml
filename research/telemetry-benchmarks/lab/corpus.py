"""Test documents with controlled compressibility.

random:  printable characters chosen uniformly; gzip saves only about 22%.
english: slices of Python docstrings (standard library and installed packages),
         which gzip about as well as ordinary prose. Lines that would trip Logfire's default
         scrubbing patterns are dropped, so scrubbing cannot hide the bytes.
"""
import random
import re
import string
from pathlib import Path

# Logfire 5.4 default scrub patterns (logfire/_internal/scrubbing.py), case-insensitive.
SCRUB = re.compile(
    r"password|passwd|mysql_pwd|secret|auth(?!ors?\b)|credential|private[._ -]?key|passphrase|"
    r"(?:signing|hmac)[._ -]?key|api[._ -]?key|session|cookie|social[._ -]?security|credit[._ -]?card|"
    r"logfire[._ -]?token|(?:access|refresh|bearer|(?:\b|_)id)[._ -]?token|pylf_v\d+_|"
    r"(?:\b|_)(?:csrf|xsrf|jwt|ssn)(?:\b|_)", re.I)

_CORPUS = None


def _corpus():
    """Unique docstring lines from every .py file in the standard library and the benchmark venv.

    Parsed with ast (nothing is imported) and cached in env/corpus.txt. About 5 MB.
    """
    global _CORPUS
    if _CORPUS is not None:
        return _CORPUS
    cache = Path(__file__).resolve().parents[1] / "env" / "corpus.txt"
    if cache.exists():
        _CORPUS = cache.read_text()
        return _CORPUS
    import ast
    import sysconfig
    roots = [Path(sysconfig.get_paths()["stdlib"]), Path(sysconfig.get_paths()["purelib"])]
    seen, lines = set(), []
    for root in roots:
        for path in sorted(root.rglob("*.py")):
            try:
                tree = ast.parse(path.read_text(errors="ignore"))
            except Exception:
                continue
            for node in ast.walk(tree):
                if isinstance(node, (ast.Module, ast.ClassDef, ast.FunctionDef, ast.AsyncFunctionDef)):
                    doc = ast.get_docstring(node)
                    if not doc:
                        continue
                    for line in doc.splitlines():
                        line = line.strip().replace("|", "/")
                        if len(line) > 20 and line.isascii() and line not in seen and not SCRUB.search(line):
                            seen.add(line)
                            lines.append(line)
    _CORPUS = "\n".join(lines)
    cache.write_text(_CORPUS)
    return _CORPUS


def document(size, seed, content="random"):
    if content == "random":
        rng = random.Random(seed)
        alphabet = string.ascii_letters + string.digits + "     "
        return "".join(rng.choice(alphabet) for _ in range(size))
    if content == "english":
        # Consecutive seeds take disjoint slices, and every line in the corpus is unique,
        # so the ten documents of one run never share text.
        text = _corpus()
        slots = len(text) // size
        start = (seed % slots) * size
        return text[start:start + size]
    raise ValueError(content)
