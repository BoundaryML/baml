"""Reference implementation. Used to validate the grader before any
agent runs. Place this as text_stats.py in the staging dir to confirm
all fixtures pass."""

import json
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: text_stats.py <path>", file=sys.stderr)
        return 2
    raw = Path(sys.argv[1]).read_bytes()
    text = raw.decode("utf-8")
    out = {
        "bytes": len(raw),
        "chars": len(text),
        "words": len(text.split()),
        "lines": text.count("\n"),
    }
    print(json.dumps(out, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
