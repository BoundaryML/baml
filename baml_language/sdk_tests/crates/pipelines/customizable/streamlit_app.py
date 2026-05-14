"""Streamlit harness for the pipelines sdk-test crate.

Lets you pick one of the BAML pipelines generated into ``baml_sdk``,
fill in its arguments, and call it. Pydantic-typed inputs render as
sub-forms with one widget per field. Each pipeline issues real
``baml.http.fetch`` calls against httpbin (delay endpoints), so plan
on ~1s per fetch — ``HandleOrder`` ends up at ~8s end to end.

Symlinked into ``generated/`` by ``sdk_test_build``. Run from
``generated/`` with::

    uv run --with streamlit streamlit run streamlit_app.py
"""

from __future__ import annotations

import json
import typing
from typing import Any, Callable

import pydantic
import streamlit as st

import baml_sdk
from baml_sdk import Cancelled, Charged, Completed, Declined, Pending


# (callable, ordered list of (arg_name, arg_type)). Hardcoded because
# the generated `_define_function` wrappers don't expose their declared
# parameter types at runtime, and inferring from `.pyi` text is brittle.
PIPELINES: dict[str, tuple[Callable[..., Any], list[tuple[str, type]]]] = {
    "HandleOrder": (baml_sdk.HandleOrder, [("id", int)]),
    "FetchOrder": (baml_sdk.FetchOrder, [("id", int)]),
    "ChargePayment": (baml_sdk.ChargePayment, [("amount", int)]),
    "ProcessPending": (baml_sdk.ProcessPending, [("p", Pending)]),
    "ProcessCompleted": (baml_sdk.ProcessCompleted, [("c", Completed)]),
}

# Order matches the .baml call graph from top to bottom — HandleOrder is
# the entry point, the rest are progressively deeper.


def _scalar_widget(label: str, ty: type, *, key: str) -> Any:
    """Render a single Streamlit widget for a scalar field."""
    if ty is int:
        return st.number_input(label, value=0, step=1, key=key)
    if ty is float:
        return st.number_input(label, value=0.0, key=key)
    if ty is bool:
        return st.checkbox(label, key=key)
    return st.text_input(label, value="", key=key)


def _model_widget(prefix: str, model_cls: type[pydantic.BaseModel]) -> pydantic.BaseModel:
    """Render one widget per declared field on a pydantic model."""
    st.markdown(f"**{prefix}** _({model_cls.__name__})_")
    values: dict[str, Any] = {}
    for fname, finfo in model_cls.model_fields.items():
        ftype = finfo.annotation
        key = f"{prefix}.{fname}"
        # Unwrap Optional[T] to T for the widget. Empty inputs stay as
        # the widget's default, not None — fine for this harness.
        origin = typing.get_origin(ftype)
        if origin is typing.Union:
            args = [a for a in typing.get_args(ftype) if a is not type(None)]
            if len(args) == 1:
                ftype = args[0]
        values[fname] = _scalar_widget(key, ftype, key=key)
    return model_cls(**values)


def _format_result(result: Any) -> None:
    if isinstance(result, pydantic.BaseModel):
        st.success(f"Returned {type(result).__name__}")
        st.json(json.loads(result.model_dump_json()))
    elif isinstance(result, str):
        st.success("Returned str")
        st.code(result, language="text")
    else:
        st.success(f"Returned {type(result).__name__}")
        st.write(result)


def main() -> None:
    st.set_page_config(page_title="BAML pipelines", layout="wide")
    st.title("BAML pipelines")
    st.caption(
        "Runs the generated SDK against real httpbin endpoints. "
        "Each `baml.http.fetch` call takes ~1s; HandleOrder hits 3 of them."
    )

    name = st.selectbox("Pipeline", list(PIPELINES), index=0)
    fn, sig = PIPELINES[name]

    st.divider()
    with st.form("invoke", clear_on_submit=False):
        args: dict[str, Any] = {}
        for arg_name, arg_ty in sig:
            if isinstance(arg_ty, type) and issubclass(arg_ty, pydantic.BaseModel):
                args[arg_name] = _model_widget(arg_name, arg_ty)
            else:
                args[arg_name] = _scalar_widget(arg_name, arg_ty, key=arg_name)
        submitted = st.form_submit_button(f"Run {name}", type="primary")

    if submitted:
        with st.spinner(f"Running {name}…"):
            try:
                result = fn(**args)
            except Exception as e:  # noqa: BLE001 — surface anything to the UI
                st.error(f"{type(e).__name__}: {e}")
                st.exception(e)
                return
        _format_result(result)


if __name__ == "__main__":
    main()


# Silence ruff unused-import on the union-arm classes — they're public
# re-exports of the BAML SDK surface and showing up here documents what
# can come back from FetchOrder / ChargePayment.
_ = (Cancelled, Charged, Declined)
