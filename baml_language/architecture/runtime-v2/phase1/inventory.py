"""Extract the user MIR and direct-call census from --rust-inventory output.

The source dump is deliberately conservative. This script does not pretend to
resolve virtual targets or specialize generic bodies; those remain compiler work.
Run through uv, with the worktree root as cwd.
"""
import argparse
import json
import re
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("dump", type=Path)
parser.add_argument("output", type=Path)
args = parser.parse_args()
sections = re.split(r"(?m)^## ", args.dump.read_text())[1:]
users = [section for section in sections if section.startswith("user.")]
mir = "".join("## " + section for section in users)
# Same-name candidate edges are not exact dispatch edges. Keep the original
# terminators and types, but omit the misleading expanded candidate lists.
mir = re.sub(r"(?m)^edge: .*\n", "", mir)
args.output.write_text("# User MIR, before native admission\n\n" + mir)
print(json.dumps({
    "functions": [section.splitlines()[0] for section in users],
    "direct_calls": sorted(set(re.findall(r"call const fn ([^<(\s]+)", mir))),
    "virtual_calls": sorted(set(re.findall(r"virtual_call (.*?) ->", mir))),
    "type_operands": sorted(set(re.findall(r"load_type\((.*?)\)", mir))),
}, indent=2))
