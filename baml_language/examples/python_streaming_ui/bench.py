"""Headless version of the Streamlit app: run the matrix and print the metrics table.

uv run python bench.py                                  # every backend x shape
uv run python bench.py --backend baml --shape structured
uv run python bench.py --render json                    # also pay json.dumps per partial, like a UI
"""

import argparse
import asyncio
import json
from pathlib import Path

from stream_ui import metrics, runners

DATA = Path(__file__).parent / "data"
DEFAULT_MODELS = {"anthropic": "claude-haiku-4-5", "openai": "gpt-4.1-mini"}


def configs(args: argparse.Namespace, pr_text: str) -> list[runners.RunConfig]:
    out = []
    for shape in args.shape:
        for backend in args.backend:
            providers = args.baml_provider if backend == "baml" else [backend]
            for provider in providers:
                out.append(runners.RunConfig(backend, shape, provider, DEFAULT_MODELS[provider], pr_text))
    return out


async def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pr", type=int, default=5041)
    parser.add_argument("--backend", nargs="+", choices=runners.BACKENDS, default=list(runners.BACKENDS))
    parser.add_argument("--shape", nargs="+", choices=runners.SHAPES, default=list(runners.SHAPES))
    parser.add_argument("--baml-provider", nargs="+", choices=["anthropic", "openai"], default=["anthropic", "openai"])
    parser.add_argument("--render", choices=["none", "json"], default="none")
    parser.add_argument("--out", type=Path, help="write summaries as JSON lines")
    args = parser.parse_args()

    pr_text = (DATA / f"pr_{args.pr}.txt").read_text()
    truth = json.loads((DATA / f"pr_{args.pr}.json").read_text())
    render = (lambda plain, _final: json.dumps(plain)) if args.render == "json" else None

    rows = []
    for cfg in configs(args, pr_text):
        m = await metrics.measure(cfg, render=render, truth=truth if cfg.shape == "structured" else None)
        row = {"shape": cfg.shape, **m.summary()}
        rows.append(row)
        print(json.dumps(row), flush=True)
    if args.out:
        args.out.write_text("".join(json.dumps(r) + "\n" for r in rows))


if __name__ == "__main__":
    asyncio.run(main())
