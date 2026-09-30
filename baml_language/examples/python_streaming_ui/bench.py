"""Headless version of the Streamlit app: run A (baml) vs B (native SDK) per scenario and print metrics.

uv run python bench.py                         # all six scenarios, A then B
uv run python bench.py --scenario 1 4          # Bedrock and Google AI Studio only
uv run python bench.py --render json           # also pay json.dumps per partial, like a UI
"""

import argparse
import asyncio
import json
from pathlib import Path

from stream_ui import metrics, runners

DATA = Path(__file__).parent / "data"


async def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pr", type=int, default=5041)
    parser.add_argument(
        "--scenario", nargs="+", type=int, choices=sorted(runners.SCENARIOS), default=sorted(runners.SCENARIOS)
    )
    parser.add_argument("--side", nargs="+", choices=runners.SIDES, default=list(runners.SIDES))
    parser.add_argument("--render", choices=["none", "json"], default="none")
    parser.add_argument("--out", type=Path, help="write summaries as JSON lines")
    args = parser.parse_args()

    pr_text = (DATA / f"pr_{args.pr}.txt").read_text(encoding="utf-8")
    truth = json.loads((DATA / f"pr_{args.pr}.json").read_text(encoding="utf-8"))
    render = (lambda plain, _final: json.dumps(plain)) if args.render == "json" else None

    rows = []
    for n in args.scenario:
        for side in args.side:
            m = await metrics.measure(runners.RunConfig(n, side, pr_text), render=render, truth=truth)
            row = {"scenario": n, "side": side, **m.summary()}
            rows.append(row)
            print(json.dumps(row), flush=True)
    if args.out:
        args.out.write_text("".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")


if __name__ == "__main__":
    asyncio.run(main())
