"""Streamlit app: stream the same task through baml_sdk, the openai SDK and the anthropic SDK side by side.

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

# (label, backend, provider) — the provider only matters for baml.
TARGETS = {
    "baml → anthropic": ("baml", "anthropic"),
    "baml → openai": ("baml", "openai"),
    "anthropic sdk": ("anthropic", "anthropic"),
    "openai sdk": ("openai", "openai"),
}

SHAPE_LABELS = {
    "structured": "Structured output: PullRequest",
    "text": "Single long string",
    "object": "Object with one long string: Narrative { text }",
}

st.set_page_config(page_title="BAML streaming UI", layout="wide")


# --------------------------------------------------------------------------- sidebar

with st.sidebar:
    st.header("Test")
    seeds = sorted(p.stem for p in DATA.glob("pr_*.txt"))
    seed = st.selectbox("Seed data", seeds)
    shape = st.radio("Stream shape", runners.SHAPES, format_func=SHAPE_LABELS.get)
    targets = st.multiselect("Backends", list(TARGETS), default=list(TARGETS))

    st.header("Models")
    model_for = {
        "anthropic": st.text_input("Anthropic model", "claude-haiku-4-5"),
        "openai": st.text_input("OpenAI model", "gpt-4.1-mini"),
    }

    st.header("Rendering")
    render_mode = st.radio(
        "Update the UI",
        ["every partial", "throttled", "final only"],
        help="'every partial' stresses the UI the most; 'final only' isolates stream overhead from Streamlit.",
    )
    fps = st.slider("Throttle (updates/s)", 1, 60, 15, disabled=render_mode != "throttled")
    concurrent = st.toggle(
        "Run backends concurrently",
        value=False,
        help="Concurrent looks nicer; sequential keeps CPU and loop-lag numbers independent.",
    )

pr_text = (DATA / f"{seed}.txt").read_text()
truth = json.loads((DATA / f"{seed}.json").read_text())


def make_configs() -> list[runners.RunConfig]:
    out = []
    for t in targets:
        backend, provider = TARGETS[t]
        out.append(runners.RunConfig(backend, shape, provider, model_for[provider], pr_text))
    return out


# --------------------------------------------------------------------------- rendering


def make_renderer(slot, shape: str):
    """Returns render(plain, is_final) that draws into `slot` per the sidebar settings."""
    min_gap = 1.0 / fps if render_mode == "throttled" else 0.0
    last = [0.0]

    def draw(plain) -> None:
        with slot.container():
            if shape == "structured":
                st.json(plain, expanded=2)
            elif shape == "object":
                st.caption("`{ text }`")
                st.markdown((plain or {}).get("text", "") if isinstance(plain, dict) else "")
            else:
                st.markdown(plain or "")

    def render(plain, is_final: bool) -> None:
        if render_mode == "final only" and not is_final:
            return
        now = time.perf_counter()
        if not is_final and now - last[0] < min_gap:
            return
        last[0] = now
        draw(plain)

    return render


async def run_all(configs, columns) -> list[metrics.RunMetrics]:
    jobs = []
    for cfg, col in zip(configs, columns):
        with col:
            st.subheader(cfg.label)
            status = st.empty()
            slot = st.empty()
        jobs.append((cfg, status, slot))

    async def one(cfg, status, slot):
        status.info("streaming…")
        m = await metrics.measure(
            cfg,
            render=make_renderer(slot, cfg.shape),
            truth=truth if cfg.shape == "structured" else None,
        )
        if m.error:
            status.error(m.error)
        else:
            status.success(f"{m.partials} partials · first {m.first_partial_s or 0:.2f}s · total {m.total_s:.2f}s")
        return m

    if concurrent:
        return list(await asyncio.gather(*(one(*j) for j in jobs)))
    return [await one(*j) for j in jobs]


# --------------------------------------------------------------------------- page

st.title("Streaming LLM output into a Python UI")
st.caption(
    "Same task, same model family: **baml_sdk** (`<Fn>_stream_async`, schema via `ctx.output_format()`) "
    "vs the **openai** / **anthropic** SDKs (structured outputs, partials parsed with `jiter`)."
)

tab_run, tab_prompts, tab_seed = st.tabs(["Run", "Prompts", "Seed data"])

with tab_run:
    go = st.button("Run", type="primary", disabled=not targets)
    if go:
        configs = make_configs()
        columns = st.columns(len(configs))
        st.session_state["results"] = asyncio.run(run_all(configs, columns))
        st.session_state["results_shape"] = shape

    results: list[metrics.RunMetrics] = st.session_state.get("results", [])
    if results:
        st.subheader(f"Metrics — {SHAPE_LABELS[st.session_state['results_shape']]}")
        st.dataframe(pd.DataFrame([m.summary() for m in results]).set_index("run"), width="stretch")
        st.caption(
            "**first partial**: request → first yielded value. **gap**: time between consecutive partials. "
            "**duplicates**: partials identical to the previous one. **shrinks**: partials that render smaller than "
            "the previous one (flicker). **convert**: model_dump / size bookkeeping. **render**: Streamlit calls. "
            "**loop lag**: how late a 5ms asyncio timer fired while streaming — a blocked loop is a frozen UI. "
            "**field accuracy**: share of ground-truth JSON leaves reproduced exactly."
        )
        chart = pd.DataFrame(
            [{"run": m.label, "t (s)": s.t, "rendered chars": s.size} for m in results for s in m.samples]
        )
        if not chart.empty:
            st.subheader("Rendered size over time")
            st.line_chart(chart, x="t (s)", y="rendered chars", color="run")
        gaps = pd.DataFrame([{"run": m.label, "gap (ms)": g} for m in results for g in m.gaps_ms])
        if not gaps.empty:
            st.subheader("Inter-partial gaps")
            st.scatter_chart(gaps.reset_index(), x="index", y="gap (ms)", color="run")

with tab_prompts:
    st.subheader("BAML prompt")
    st.caption("The schema is rendered by `ctx.output_format()` as a TypeScript-style object literal, not JSON Schema.")
    try:
        st.code(runners.baml_prompt(shape, pr_text), language="markdown")
    except Exception as e:
        st.error(f"{type(e).__name__}: {e}")
    st.subheader("SDK prompt")
    st.code(runners.sdk_prompt(shape, pr_text), language="markdown")
    output_model = {"structured": models.PullRequest, "object": models.Narrative}.get(shape)
    if output_model is not None:
        st.subheader("SDK structured-output schema")
        st.json(output_model.model_json_schema(), expanded=False)

with tab_seed:
    left, right = st.columns(2)
    with left:
        st.subheader("Plaintext (model input)")
        st.code(pr_text, language="markdown")
    with right:
        st.subheader("JSON (ground truth)")
        st.json(truth, expanded=1)
