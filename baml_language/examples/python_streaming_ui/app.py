"""Streamlit app: A vs B structured-output streaming for the six (provider, v1 client) scenarios.

A = baml_sdk (v1) through the scenario's v1 client; B = the provider's native Python SDK; C = BAML v0 (baml-py).

uv run streamlit run app.py
"""

import asyncio
import json
import time
from pathlib import Path

import pandas as pd
import streamlit as st

from stream_ui import metrics, models, runners

DATA = Path(__file__).parent / "data"
SIDE_NAME = {"baml": "A · baml v1", "native": "B · native SDK", "v0": "C · baml v0"}

# Metrics shown side by side in the A vs B table.
COMPARE = [
    "first partial (s)",
    "total (s)",
    "partials/s",
    "gap p95 (ms)",
    "gap max (ms)",
    "duplicates",
    "cpu (s)",
    "loop lag max (ms)",
    "field accuracy",
    "error",
]

st.set_page_config(page_title="BAML vs native SDK streaming", layout="wide")


def scenario_title(n: int) -> str:
    s = runners.SCENARIOS[n]
    return f"{n} · {s.provider} · {s.v1_client}"


# --------------------------------------------------------------------------- sidebar

with st.sidebar:
    st.header("Scenario")
    # One at a time: each scenario is three long streams.
    chosen = [st.selectbox("Run", sorted(runners.SCENARIOS), format_func=scenario_title)]
    for n in chosen:
        if missing := runners.SCENARIOS[n].missing_env():
            st.warning(f"{n}: missing {', '.join(missing)}")
    sides = [side for side in runners.SIDES if st.checkbox(SIDE_NAME[side], value=True)]
    order = st.radio(
        "Within a scenario",
        ["in order (A, B, C)", "all at once"],
        help="In order keeps CPU and loop-lag numbers independent; all at once is for watching them race.",
    )

    st.header("Rendering")
    render_mode = st.radio(
        "Update the UI",
        ["every partial", "throttled", "final only"],
        help="'every partial' stresses the UI the most; 'final only' isolates stream overhead from Streamlit.",
    )
    fps = st.slider("Throttle (updates/s)", 1, 60, 15, disabled=render_mode != "throttled")

pr_text = (DATA / "pr_5041.txt").read_text(encoding="utf-8")
truth = json.loads((DATA / "pr_5041.json").read_text(encoding="utf-8"))


# --------------------------------------------------------------------------- running


def make_renderer(slot):
    """Returns render(plain, is_final) that draws into `slot` per the sidebar settings."""
    min_gap = 1.0 / fps if render_mode == "throttled" else 0.0
    last = [0.0]

    def render(plain, is_final: bool) -> None:
        if render_mode == "final only" and not is_final:
            return
        now = time.perf_counter()
        if not is_final and now - last[0] < min_gap:
            return
        last[0] = now
        slot.json(plain, expanded=2)

    return render


async def run_one(cfg: runners.RunConfig, status, slot) -> metrics.RunMetrics:
    status.info("streaming…")
    m = await metrics.measure(cfg, render=make_renderer(slot), truth=truth)
    if m.error:
        status.error(m.error)
    else:
        status.success(
            f"{m.partials} partials · first {m.first_partial_s or 0:.2f}s · total {m.total_s:.2f}s"
            f" · accuracy {m.accuracy:.2f}"
        )
    return m


async def run_all() -> list[tuple[int, str, metrics.RunMetrics]]:
    results = []
    for n in chosen:
        s = runners.SCENARIOS[n]
        st.subheader(scenario_title(n))
        st.caption(f"`{s.representative}` · `{s.model}` · B uses {s.native_sdk}")
        columns = dict(zip(sides, st.columns(len(sides))))
        jobs = []
        for side in sides:
            with columns[side]:
                st.markdown(f"**{SIDE_NAME[side]}**")
                status, slot = st.empty(), st.empty()
            jobs.append((side, runners.RunConfig(n, side, pr_text), status, slot))
        if order == "all at once":
            done = await asyncio.gather(*(run_one(cfg, status, slot) for _, cfg, status, slot in jobs))
        else:
            done = [await run_one(cfg, status, slot) for _, cfg, status, slot in jobs]
        results += [(n, side, m) for (side, *_), m in zip(jobs, done)]
    return results


def comparison_table(results) -> pd.DataFrame:
    rows = [{"scenario": scenario_title(n), "side": side, **m.summary()} for n, side, m in results]
    df = pd.DataFrame(rows)
    wide = df.pivot(index="scenario", columns="side", values=COMPARE)
    wide.columns = [f"{metric} · {runners.SIDE_LETTER[side]}" for metric, side in wide.columns]
    ordered = [f"{metric} · {x}" for metric in COMPARE for x in ("A", "B", "C") if f"{metric} · {x}" in wide.columns]
    return wide[ordered]


# --------------------------------------------------------------------------- page

st.title("Structured-output streaming: BAML v1 vs native SDKs vs BAML v0")
st.caption(
    "Each scenario rebuilds the `gh pr view --json` object for BoundaryML/baml#5041 from a prose rendering of it. "
    "**A** streams `ExtractPullRequest` through BAML with the scenario's v1 client (schema via "
    "`ctx.output_format()`). **B** streams the same instructions through the provider's native Python SDK with "
    "structured outputs, parsing partials with `jiter`. **C** streams the same function through BAML v0 "
    "(`baml-py`) with the equivalent v0 `client<llm>`."
)

tab_run, tab_scenarios, tab_prompts, tab_seed = st.tabs(["Run", "Scenarios", "Prompts", "Seed data"])

with tab_run:
    if st.button("Run", type="primary", disabled=not (chosen and sides)):
        st.session_state["results"] = asyncio.run(run_all())

    results = st.session_state.get("results", [])
    if results:
        st.header("A vs B vs C")
        st.dataframe(comparison_table(results), width="stretch")
        st.caption(
            "**first partial**: request → first yielded value. **gap**: time between consecutive partials. "
            "**duplicates**: partials identical to the previous one. **cpu**: process CPU time for the run. "
            "**loop lag**: how late a 5ms asyncio timer fired while streaming — a blocked loop is a frozen UI. "
            "**field accuracy**: share of ground-truth JSON leaves reproduced exactly."
        )
        with st.expander("All metrics"):
            st.dataframe(
                pd.DataFrame([{"scenario": n, "side": side, **m.summary()} for n, side, m in results]),
                width="stretch",
            )
        chart = pd.DataFrame(
            [{"run": m.label, "t (s)": s.t, "rendered chars": s.size} for _, _, m in results for s in m.samples]
        )
        if not chart.empty:
            st.subheader("Rendered size over time")
            st.line_chart(chart, x="t (s)", y="rendered chars", color="run")

with tab_scenarios:
    st.dataframe(
        pd.DataFrame(
            [
                {
                    "#": s.n,
                    "provider": s.provider,
                    "v1 client (A)": s.v1_client,
                    "representative": s.representative,
                    "model": s.model,
                    "native SDK (B)": s.native_sdk,
                    "v0 client (C)": s.representative,
                    "env": "ok" if not s.missing_env() else "missing " + ", ".join(s.missing_env()),
                }
                for s in runners.SCENARIOS.values()
            ]
        ).set_index("#"),
        width="stretch",
    )

with tab_prompts:
    left, right = st.columns(2)
    with left:
        st.subheader("A · BAML prompt")
        st.caption("The schema is rendered by `ctx.output_format()` as a TypeScript-style object literal.")
        try:
            st.code(runners.baml_prompt(pr_text), language="markdown")
        except Exception as e:
            st.error(f"{type(e).__name__}: {e}")
    with right:
        st.subheader("B · native SDK prompt")
        st.code(runners.native_prompt(pr_text), language="markdown")
        st.subheader("B · structured-output schema")
        st.json(models.PullRequest.model_json_schema(), expanded=False)

with tab_seed:
    left, right = st.columns(2)
    with left:
        st.subheader("Plaintext (model input)")
        st.code(pr_text, language="markdown")
    with right:
        st.subheader("JSON (ground truth)")
        st.json(truth, expanded=1)
