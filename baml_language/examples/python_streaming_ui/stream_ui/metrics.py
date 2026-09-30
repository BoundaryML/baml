"""Time a stream end to end, as a UI consuming it would see it."""

from __future__ import annotations

import asyncio
import json
import statistics
import time
from dataclasses import dataclass, field
from typing import Any, Callable, Optional

from pydantic import BaseModel

from . import runners


def to_plain(value: Any) -> Any:
    """Partial value -> JSON-able data (what a UI would render)."""
    if isinstance(value, BaseModel):
        return value.model_dump(mode="json")
    return value


def rendered_size(plain: Any) -> int:
    if plain is None:
        return 0
    if isinstance(plain, str):
        return len(plain)
    return len(json.dumps(plain, ensure_ascii=False))


@dataclass
class Sample:
    t: float  # seconds since the request started
    size: int  # rendered characters


@dataclass
class RunMetrics:
    label: str
    samples: list[Sample] = field(default_factory=list)
    first_partial_s: Optional[float] = None
    total_s: Optional[float] = None
    partials: int = 0
    # Consecutive partials that render identically: wasted UI work.
    duplicate_partials: int = 0
    # Partials that render smaller than the one before: visible flicker.
    shrinking_partials: int = 0
    # Time this process spent converting partials to plain data (model_dump / partial JSON parse is upstream).
    convert_s: float = 0.0
    # Time the UI callback took (Streamlit rendering).
    render_s: float = 0.0
    cpu_s: float = 0.0
    loop_lag_max_ms: float = 0.0
    loop_lag_p95_ms: float = 0.0
    final: Any = None
    accuracy: Optional[float] = None
    error: Optional[str] = None

    @property
    def gaps_ms(self) -> list[float]:
        ts = [s.t for s in self.samples]
        return [(b - a) * 1000 for a, b in zip(ts, ts[1:])]

    def summary(self) -> dict[str, Any]:
        gaps = self.gaps_ms
        stream_s = (self.samples[-1].t - self.samples[0].t) if len(self.samples) > 1 else 0.0
        last_partial_size = self.samples[-1].size if self.samples else 0
        final_size = rendered_size(to_plain(self.final)) if self.final is not None else last_partial_size
        return {
            "run": self.label,
            "first partial (s)": _r(self.first_partial_s),
            "total (s)": _r(self.total_s),
            "partials": self.partials,
            "partials/s": _r(self.partials / stream_s if stream_s else None, 1),
            "chars/s": _r(last_partial_size / stream_s if stream_s else None, 0),
            "gap p50 (ms)": _r(statistics.median(gaps) if gaps else None, 1),
            "gap p95 (ms)": _r(_pct(gaps, 95), 1),
            "gap max (ms)": _r(max(gaps) if gaps else None, 1),
            "duplicates": self.duplicate_partials,
            "shrinks": self.shrinking_partials,
            "convert (ms)": _r(self.convert_s * 1000, 1),
            "render (ms)": _r(self.render_s * 1000, 1),
            "cpu (s)": _r(self.cpu_s),
            "loop lag p95 (ms)": _r(self.loop_lag_p95_ms, 1),
            "loop lag max (ms)": _r(self.loop_lag_max_ms, 1),
            "final chars": final_size,
            "field accuracy": _r(self.accuracy),
            "error": self.error,
        }


def _r(v: Optional[float], nd: int = 2) -> Optional[float]:
    return None if v is None else round(v, nd)


def _pct(xs: list[float], p: float) -> Optional[float]:
    if not xs:
        return None
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(round(p / 100 * (len(xs) - 1))))]


class LoopLagMonitor:
    """Samples how late the event loop wakes a 5ms timer: a blocked loop is a frozen UI."""

    def __init__(self, interval_s: float = 0.005):
        self.interval_s = interval_s
        self.lags_ms: list[float] = []
        self._task: Optional[asyncio.Task] = None

    async def _run(self) -> None:
        while True:
            t0 = time.perf_counter()
            await asyncio.sleep(self.interval_s)
            self.lags_ms.append(max(0.0, (time.perf_counter() - t0 - self.interval_s) * 1000))

    async def __aenter__(self) -> "LoopLagMonitor":
        self._task = asyncio.get_running_loop().create_task(self._run())
        return self

    async def __aexit__(self, *exc: Any) -> None:
        assert self._task is not None
        # Yield once more so a wake-up delayed by the final render gets recorded.
        await asyncio.sleep(self.interval_s)
        self._task.cancel()


def _leaves(value: Any, path: str = "") -> dict[str, Any]:
    if isinstance(value, dict):
        out: dict[str, Any] = {}
        for k, v in value.items():
            out.update(_leaves(v, f"{path}.{k}"))
        return out
    if isinstance(value, list):
        out = {f"{path}.len": len(value)}
        for i, v in enumerate(value):
            out.update(_leaves(v, f"{path}[{i}]"))
        return out
    return {path: value}


def field_accuracy(final: Any, truth: Any) -> float:
    """Fraction of ground-truth leaf values reproduced exactly."""
    want, got = _leaves(truth), _leaves(to_plain(final))
    return sum(1 for k, v in want.items() if got.get(k) == v) / len(want)


RenderFn = Callable[[Any, bool], None]


async def measure(
    cfg: runners.RunConfig,
    render: Optional[RenderFn] = None,
    truth: Any = None,
) -> RunMetrics:
    """Consume one stream; `render(plain_value, is_final)` is called for every partial."""
    m = RunMetrics(label=cfg.label)
    prev: Any = object()
    prev_size = 0
    cpu0 = time.process_time()
    t0 = time.perf_counter()
    async with LoopLagMonitor() as lag:
        try:
            async for item in runners.stream(cfg):
                if isinstance(item, runners.Partial):
                    m, prev, prev_size = _on_partial(m, item.value, t0, prev, prev_size, render)
                    continue
                m.total_s = time.perf_counter() - t0
                m.final = item.value
                if render is not None:
                    r0 = time.perf_counter()
                    render(to_plain(item.value), True)
                    m.render_s += time.perf_counter() - r0
        except Exception as e:  # surfaced in the UI table
            m.total_s = time.perf_counter() - t0
            m.error = f"{type(e).__name__}: {e}"
    m.cpu_s = time.process_time() - cpu0
    if lag.lags_ms:
        m.loop_lag_max_ms = max(lag.lags_ms)
        m.loop_lag_p95_ms = _pct(lag.lags_ms, 95) or 0.0
    if truth is not None and m.final is not None and m.error is None:
        m.accuracy = field_accuracy(m.final, truth)
    return m


def _on_partial(m: RunMetrics, value: Any, t0: float, prev: Any, prev_size: int, render: Optional[RenderFn]):
    t = time.perf_counter() - t0
    c0 = time.perf_counter()
    plain = to_plain(value)
    size = rendered_size(plain)
    m.convert_s += time.perf_counter() - c0
    if m.first_partial_s is None:
        m.first_partial_s = t
    m.partials += 1
    if plain == prev:
        m.duplicate_partials += 1
    if size < prev_size:
        m.shrinking_partials += 1
    m.samples.append(Sample(t=t, size=size))
    if render is not None:
        r0 = time.perf_counter()
        render(plain, False)
        m.render_s += time.perf_counter() - r0
    return m, plain, size
