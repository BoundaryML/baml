"""Host tracing: theoretical Python examples to audit before implementation.

These are ordinary functions, decorators, calls, and assertions against the
intended API. There is no runner or observation API. Call a scenario individually
when the implementation exists. Expected recorder behavior is written beside
its calls; it is part of the spec even where Python assertions cannot inspect it.

Assume a generated BAML project exports baml_sdk.trace and the tracing_examples
namespace imported below. Its small, deterministic BAML functions are:

  echo(value: int) -> int
      Return value.
  read_context() -> trace.Context
      Return trace.current_context() from inside that BAML invocation.
  middle() -> trace.Context
      Call read_context($trace = trace.span()).
  call(callback: (int) -> int, value: int) -> int
      Invoke callback(value) once, then return its result.
  call_configured(callback: (int) -> int, value: int,
                  options: trace.Options | trace.ReservedSpan) -> int
      Invoke callback(value, $trace = options) once, then return its result.
  call_then_context(callback: (int) -> int, value: int,
                    options: trace.Options) -> trace.Context
      Invoke callback(value, $trace = options), then return the BAML caller's
      own trace.current_context().
  reserve_callback(callback: (int) -> int) -> trace.SpanId
      Reserve a span in BAML, attach it to callback(1), and return its ID.
  identity_span_id(value: trace.SpanId) -> trace.SpanId
      Return value unchanged.
  stream_text(value: str) -> str
      An AI function backed by a deterministic scripted streaming client.
      Its generated stream_text_stream_async entrypoint returns a native BAML
      stream with nonempty string prefixes and final value equal to value.
      Use its ordinary async iteration and final_async() accessors below.

Each has its normal generated *_async entrypoint and _baml invocation option (with trace in its trace field).
Their default mode is Span. No LLM/network work. These are application BAML
functions, not new bridge helpers or hand-written Python substitutes.

'Proposed' and 'optional' comments mark unsettled support, not implicit promises.
Recording policy (enabled/disabled, Timing retention, automatic capture) is
selected through existing configuration before running the relevant example.
Python >= 3.11 is assumed for the TaskGroup example. No public flush API.
"""

from __future__ import annotations

import asyncio
import contextvars
import functools
import multiprocessing
import threading
from concurrent.futures import ThreadPoolExecutor
from typing import AsyncGenerator, Generator

from baml_sdk import trace
from baml_sdk import tracing_examples as baml


# API-01: The same generated Options configures a host and a BAML invocation.
def scenario_shared_host_and_baml_options():
    options = trace.span(inputs=True, output=True).context(
        metadata={"component": "api"}
    )

    @trace.instrument(options, name="request")
    def request(value: int) -> int:
        return baml.echo(value, _baml={"trace": options})

    assert request(7) == 7
    # Recorded: request -> echo. Both carry component=api and capture output=7.
    # Their IDs differ. trace.span is a builder; trace.instrument marks execution.


# API-02: All supported decorator forms use Span when mode is unspecified.
def scenario_decorator_forms():
    @trace.instrument
    def bare():
        return 1

    @trace.instrument()
    def factory():
        return 2

    @trace.instrument(None)
    def null_options():
        return 3

    @trace.instrument(trace.options(), name="display")
    def explicit_options():
        return 4

    assert (bare(), factory(), null_options(), explicit_options()) == (1, 2, 3, 4)
    # Four separate roots. Default names use qualname; last uses 'display'.


# API-03: Signature, result identity, and synchronous execution stay ordinary.
def scenario_arguments_and_sync_execution():
    caller_thread = threading.get_ident()
    result = object()

    @trace.instrument
    def work(first, /, second=2, *rest, flag=True, **kwargs):
        assert threading.get_ident() == caller_thread
        assert (first, second, rest, flag, kwargs) == (
            1,
            3,
            (4, 5),
            False,
            {"extra": 6},
        )
        return result

    assert work(1, 3, 4, 5, flag=False, extra=6) is result
    # Preserve ordinary argument-binding errors and exceptions as well.
    # Arbitrary callable attributes/function-object identity are not promised.


# API-04: A host parameter named _baml remains application data.
def scenario_application_trace_parameter():
    value = object()

    @trace.instrument
    def work(*, _baml):
        return _baml

    assert work(_baml=value) is value
    # Generated BAML calls have their _baml option; host calls add no such option.


# API-03/API-05: Bound methods; proposed static/class descriptor ordering.
def scenario_methods():
    class Service:
        def __init__(self, prefix):
            self.prefix = prefix

        @trace.instrument(name="process")
        def process(self, value):
            return self.normalize(value)

        @trace.instrument(name="normalize")
        def normalize(self, value):
            return self.prefix + value.strip()

        @staticmethod
        @trace.instrument(name="static")
        def static(value):
            return value + 1

        @classmethod
        @trace.instrument(name="class")
        def make(cls):
            return cls("hello ")

    assert Service.make().process(" world ") == "hello world"
    assert Service.static(6) == 7
    # process -> normalize. Binding preserves self/cls.
    # Decide descriptor-order support; the examples place instrument inside.


# API-06: An ordinary function returning an iterator is an ordinary invocation.
def scenario_returning_iterator():
    iterator = iter([1, 2])

    @trace.instrument
    def make_iterator():
        return iterator

    assert make_iterator() is iterator
    assert next(iterator) == 1
    assert next(iterator) == 2
    # Span ends at make_iterator's return. No consumption or iteration tracing.


# CTX-01: Building options/wrappers has no ambient effect or invocation.
def scenario_inert_construction():
    assert trace.current_context().distinct_id is None
    assert trace.current_context().metadata == {}
    options = trace.context(distinct_id="unused", metadata={"request": "unused"})

    @trace.instrument(options)
    def never_called():
        raise AssertionError("Body must not run")

    assert callable(never_called)
    assert trace.current_context().distinct_id is None
    assert trace.current_context().metadata == {}
    # No invocations or recorded events for construction.


# CTX-02: Builders leave their receiver unchanged; order selects mode.
def scenario_builder_composition():
    base = trace.context(metadata={"stage": "prepare"})
    span = base.timing().span(output=True)
    timing = span.mode(trace.Mode.Timing)
    default = timing.mode(None)

    assert base.inspect().mode is None
    assert span.inspect().mode == trace.Mode.Span
    assert span.inspect().capture.output is True
    assert timing.inspect().mode == trace.Mode.Timing
    assert timing.inspect().capture.output is True
    assert default.inspect().mode is None
    assert default.inspect().context.metadata == {"stage": "prepare"}
    # context changes neither mode nor capture. None restores the callee default.


# CTX-03: Supplied maps and inspection/context snapshots are detached.
def scenario_detached_snapshots():
    supplied = {"tenant": "acme"}
    options = trace.context(metadata=supplied)
    supplied["tenant"] = "changed outside"
    options.inspect().context.metadata["tenant"] = "changed snapshot"
    assert options.inspect().context.metadata == {"tenant": "acme"}

    @trace.instrument(options)
    def work():
        snapshot = trace.current_context()
        snapshot.metadata["tenant"] = "edited"
        assert trace.current_context().metadata == {"tenant": "acme"}

    work()
    assert trace.current_context().metadata == {}


# CTX-04: Merge/overwrite/remove metadata; null identity inherits.
def scenario_context_inheritance():
    @trace.instrument(
        trace.context(distinct_id="alice", metadata={"keep": 1, "drop": 2})
    )
    def parent():
        first = baml.read_context(
            _baml={
                "trace": trace.context(
                    distinct_id=None, metadata={"drop": None, "new": True}
                )
            }
        )
        assert first.distinct_id == "alice"
        assert first.metadata == {"keep": 1, "new": True}

        second = baml.read_context(_baml={"trace": trace.context(distinct_id="bob")})
        assert second.distinct_id == "bob"
        assert second.metadata == {"keep": 1, "drop": 2}

        third = baml.read_context(_baml={"trace": trace.context()})
        assert third.distinct_id == "alice"
        assert third.metadata == {"keep": 1, "drop": 2}
        assert trace.current_context().metadata == {"keep": 1, "drop": 2}

    parent()
    # Three siblings; parent context never changes. Null removes only on the child.


# CTX-05: An uninstrumented helper can return options, not set context.
def scenario_options_from_helper():
    def settings():
        return trace.context(metadata={"stage": "selected"})

    @trace.instrument(trace.context(metadata={"stage": "parent"}))
    def parent():
        options = settings()
        assert trace.current_context().metadata == {"stage": "parent"}
        assert baml.read_context(_baml={"trace": options}).metadata == {
            "stage": "selected"
        }
        assert baml.read_context().metadata == {"stage": "parent"}

    parent()
    # settings is not a boundary. Both BAML invocations belong to parent.


# CTX-06: Identity discovered during execution configures a new invocation.
def scenario_identity_learned_later():
    @trace.instrument
    def request():
        user_id = "user-7"  # Application authentication result.

        @trace.instrument(trace.context(distinct_id=user_id))
        def authenticated_work():
            return baml.read_context()

        assert authenticated_work().distinct_id == user_id
        assert trace.current_context().distinct_id is None
        assert baml.read_context().distinct_id is None

    request()
    # Only authenticated_work and its descendants get user-7.


# CTX-07: Wrapper creation never captures its current parent.
def scenario_wrapper_creation_context():
    @trace.instrument(trace.context(metadata={"request": "A"}), name="creator")
    def creator():
        @trace.instrument(name="child")
        def child():
            return trace.current_context()

        return child

    @trace.instrument(trace.context(metadata={"request": "B"}), name="caller")
    def caller(child):
        return child()

    assert caller(creator()).metadata == {"request": "B"}
    # creator and caller are separate roots. caller -> child, never creator -> child.


# CTX-08: Reusing Options never reuses identities or attaches a previous parent.
def scenario_reused_options():
    options = trace.span(output=True).context(metadata={"component": "extract"})

    @trace.instrument(options, name="prepare")
    def prepare(value):
        return baml.echo(value, _baml={"trace": options})

    assert prepare(1) == 1
    assert prepare(2) == 2
    # Four distinct IDs: prepare1 -> echo1 and prepare2 -> echo2.


# CTX-09: Run with recording disabled through existing configuration.
def scenario_recording_disabled():
    @trace.instrument(trace.context(metadata={"stage": "host"}))
    def parent():
        assert baml.read_context(
            _baml={"trace": trace.context(metadata={"stage": "baml"})}
        ).metadata == {"stage": "baml"}
        assert trace.current_context().metadata == {"stage": "host"}

    parent()
    assert trace.current_context().metadata == {}
    # No records. Context boundaries/builders/inspection still have full semantics.


# CTX-10: Parent context is fixed across child errors and suspension.
async def scenario_fixed_context_after_child_error():
    failure = ValueError("child failed")

    @trace.instrument(trace.context(metadata={"stage": "child"}))
    async def child():
        raise failure

    @trace.instrument(trace.context(metadata={"stage": "parent"}))
    async def parent():
        before = trace.current_context()
        try:
            await child()
        except ValueError as caught:
            assert caught is failure
        else:
            raise AssertionError("Expected child failure")
        await asyncio.sleep(0)
        assert (
            trace.current_context().metadata == before.metadata == {"stage": "parent"}
        )

    await parent()
    # Child error; parent success. Entry/exit context equal for each invocation.


# MODE-01: Hidden calls omit spans but retain their own fixed contexts.
def scenario_hidden_host_and_baml():
    @trace.instrument(trace.hidden().context(metadata={"host_stage": "hidden"}))
    def hidden_host():
        return baml.middle(
            _baml={"trace": trace.hidden().context(metadata={"baml_stage": "hidden"})}
        )

    @trace.instrument(trace.context(metadata={"request": "r1"}), name="parent")
    def parent():
        assert hidden_host().metadata == {
            "request": "r1",
            "host_stage": "hidden",
            "baml_stage": "hidden",
        }
        assert baml.read_context().metadata == {"request": "r1"}

    parent()
    # Logical: parent -> hidden_host -> middle -> read_context, plus sibling leaf.
    # Recorded: both leaves attach to parent; neither hidden invocation has a span.


# MODE-02/MODE-03: Run with prepare retention off, then with late retention on.
def scenario_timing_retention():
    @trace.instrument(
        trace.timing().context(metadata={"stage": "prepare"}), name="prepare"
    )
    def prepare():
        return baml.read_context()

    @trace.instrument(name="request")
    def request():
        assert prepare().metadata == {"stage": "prepare"}
        assert baml.echo(7) == 7

    request()
    # No retention: request -> read_context and request -> echo; prepare has stats.
    # Late retention: request -> prepare -> read_context; request -> echo.
    # Identical execution/context. Retention must reconstruct parentage and count once.


# MODE-04: Timing-only root must not force an individual root span.
def scenario_unretained_root():
    @trace.instrument(
        trace.timing().context(metadata={"request": "r1"}), name="request"
    )
    def request():
        assert baml.echo(1) == 1
        assert baml.echo(2) == 2

    request()
    # With no request retention, echoes can be separate recorded roots.
    # Logical ancestry/stats still include request. Do not invent a connecting span.


# COMP-01: Explicit stacked wrappers remain two boundaries.
def scenario_stacked_instrumentation():
    @trace.instrument(trace.context(metadata={"stage": "outer"}), name="outer")
    @trace.instrument(trace.context(metadata={"stage": "inner"}), name="inner")
    def work():
        assert trace.current_context().metadata == {"stage": "inner"}
        return baml.read_context()

    assert work().metadata == {"stage": "inner"}
    # outer -> inner -> read_context. Outer context remains stage=outer.


# COMP-02: This retry is ordinary application code, not tracing machinery.
def scenario_retry_order():
    def retry_once(fn):
        @functools.wraps(fn)
        def wrapped(*args, **kwargs):
            try:
                return fn(*args, **kwargs)
            except ValueError:
                return fn(*args, **kwargs)

        return wrapped

    attempts = {"whole": 0, "each": 0}

    def body(key):
        attempts[key] += 1
        if attempts[key] == 1:
            raise ValueError("retry")
        return 7

    whole = trace.instrument(retry_once(body), name="whole")
    each = retry_once(trace.instrument(body, name="attempt"))
    assert whole("whole") == each("each") == 7
    # whole: one successful invocation. each: errored attempt then successful attempt.


# COMP-03: Cache order follows normal decorator composition.
def scenario_cache_order():
    @trace.instrument(name="outside")
    @functools.lru_cache()
    def outside(value):
        return value

    @functools.lru_cache()
    @trace.instrument(name="inside")
    def inside(value):
        return value

    assert outside(1) == outside(1) == inside(1) == inside(1) == 1
    # Two outside invocations; one inside invocation. No synthetic body on a hit.


# ASYNC-01: Instrumentation awaits inline in the invoking task/loop.
async def scenario_async_execution_environment():
    caller_task = asyncio.current_task()
    caller_loop = asyncio.get_running_loop()
    application_context = contextvars.ContextVar("application", default="before")
    result = object()

    @trace.instrument
    async def work():
        assert asyncio.current_task() is caller_task
        assert asyncio.get_running_loop() is caller_loop
        application_context.set("changed by body")
        await asyncio.sleep(0)
        assert asyncio.current_task() is caller_task
        return result

    assert await work() is result
    assert application_context.get() == "changed by body"
    assert trace.current_context().metadata == {}
    # Restore tracing's frame only; do not restore unrelated application ContextVars.


# ASYNC-02: Coroutine creation captures nothing; first execution captures inputs.
async def scenario_coroutine_creation():
    values = [1]

    @trace.instrument(trace.span(inputs=True, output=True), name="work")
    async def work(items):
        return sum(items)

    coroutine = work(values)
    values.append(2)
    assert await coroutine == 3
    # No invocation at creation; recorded input [1, 2], output 3 at execution.
    never_started = work(values)
    never_started.close()
    # No invocation for never_started.


# ASYNC-03: Create under A, first execute under B.
async def scenario_coroutine_first_execution():
    @trace.instrument(name="work")
    async def work():
        return trace.current_context()

    @trace.instrument(trace.context(metadata={"request": "A"}), name="create")
    def create():
        return work()

    @trace.instrument(trace.context(metadata={"request": "B"}), name="consume")
    async def consume(coroutine):
        return await coroutine

    assert (await consume(create())).metadata == {"request": "B"}
    # Separate create/consume roots. consume -> work.


# ASYNC-04: Detached task inherits its launch frame even after parent returns.
async def scenario_child_outlives_parent():
    release = asyncio.Event()

    @trace.instrument(name="child")
    async def child():
        await release.wait()
        assert trace.current_context().metadata == {"request": "A"}
        return await baml.echo_async(7)

    @trace.instrument(trace.context(metadata={"request": "A"}), name="launch")
    async def launch():
        return asyncio.create_task(child())

    task = await launch()
    # launch has ended. Child stays active/inherited; parent is not extended/reopened.
    release.set()
    assert await task == 7
    # launch -> child -> echo, regardless of ordinary versus eager task start.


# ASYNC-05: Awaiting running work under B cannot reparent its invocation from A.
async def scenario_awaiting_running_task():
    entered, release = asyncio.Event(), asyncio.Event()

    @trace.instrument(name="child")
    async def child():
        entered.set()
        await release.wait()
        return trace.current_context()

    @trace.instrument(trace.context(metadata={"request": "A"}), name="launch")
    async def launch():
        return asyncio.create_task(child())

    @trace.instrument(trace.context(metadata={"request": "B"}), name="consume")
    async def consume(task):
        release.set()
        result = await task
        assert trace.current_context().metadata == {"request": "B"}
        return result

    task = await launch()
    await entered.wait()
    assert (await consume(task)).metadata == {"request": "A"}
    # launch -> child; consume is a separate root.


# ASYNC-06: Force two request roots and four child invocations to overlap.
async def scenario_concurrent_requests():
    arrivals = 0
    all_entered = asyncio.Event()

    async def request(request_id):
        @trace.instrument(
            trace.context(metadata={"request": request_id}), name="request"
        )
        async def body():
            async def run_child(label):
                @trace.instrument(
                    trace.context(metadata={"child": label}), name="child"
                )
                async def child():
                    nonlocal arrivals
                    arrivals += 1
                    if arrivals == 4:
                        all_entered.set()
                    await all_entered.wait()
                    expected = {"request": request_id, "child": label}
                    assert trace.current_context().metadata == expected
                    assert (await baml.read_context_async()).metadata == expected
                    assert trace.current_context().metadata == expected

                await child()

            await asyncio.gather(run_child("left"), run_child("right"))
            assert trace.current_context().metadata == {"request": request_id}

        await body()

    await asyncio.gather(request("A"), request("B"))
    assert trace.current_context().metadata == {}
    # Two roots, each with two children and their own BAML leaves; distinct IDs.


# ASYNC-07: The host's explicitly empty task context starts an independent root.
async def scenario_explicit_task_context():
    @trace.instrument(name="child")
    async def child():
        return trace.current_context()

    @trace.instrument(trace.context(metadata={"request": "A"}))
    async def parent():
        task = contextvars.Context().run(asyncio.create_task, child())
        assert (await task).metadata == {}
        assert trace.current_context().metadata == {"request": "A"}

    await parent()
    # parent and child are separate roots. Explicit copied context would inherit.


# ASYNC-08: A live body has no terminal event just because records are observed.
async def scenario_live_lifetime():
    entered, release = asyncio.Event(), asyncio.Event()

    @trace.instrument(name="work")
    async def work():
        entered.set()
        await release.wait()
        return 7

    task = asyncio.create_task(work())
    await entered.wait()
    assert not task.done()
    # Recorder may already contain work's start; must contain no completion yet.
    release.set()
    assert await task == 7
    # Exactly one successful completion after body exit. No flush API involved.


# ASYNC-09: Top-level gather creates independent roots without a traced parent.
async def scenario_concurrent_independent_roots():
    arrivals = 0
    all_entered = asyncio.Event()
    assert trace.current_context().metadata == {}

    async def run(job):
        @trace.instrument(trace.context(metadata={"job": job}), name="job")
        async def root():
            nonlocal arrivals
            arrivals += 1
            if arrivals == 10:
                all_entered.set()
            await all_entered.wait()
            expected = {"job": job}
            assert trace.current_context().metadata == expected
            assert (await baml.read_context_async()).metadata == expected
            assert trace.current_context().metadata == expected
            return job

        result = await root()
        assert trace.current_context().metadata == {}
        return result

    assert await asyncio.gather(*(run(job) for job in range(10))) == list(range(10))
    assert trace.current_context().metadata == {}
    # Ten independent job roots, each with one read_context child. Twenty distinct
    # invocation IDs, twenty starts and twenty successful completions. No job is
    # another job's parent; gather itself creates no tracing boundary.


# ASYNC-10: One async parent mixes async host, sync host, and BAML children.
async def scenario_mixed_children_under_async_parent():
    caller_thread = threading.get_ident()
    caller_task = asyncio.current_task()

    @trace.instrument(trace.context(metadata={"kind": "async"}), name="async_child")
    async def async_child():
        assert asyncio.current_task() is caller_task
        await asyncio.sleep(0)
        assert trace.current_context().metadata == {"request": "A", "kind": "async"}
        return 1

    @trace.instrument(trace.context(metadata={"kind": "sync"}), name="sync_child")
    def sync_child():
        assert threading.get_ident() == caller_thread
        assert trace.current_context().metadata == {"request": "A", "kind": "sync"}
        return 2

    @trace.instrument(trace.context(metadata={"request": "A"}), name="parent")
    async def parent():
        assert await async_child() == 1
        assert trace.current_context().metadata == {"request": "A"}
        assert sync_child() == 2
        assert trace.current_context().metadata == {"request": "A"}
        assert (await baml.read_context_async()).metadata == {"request": "A"}
        assert trace.current_context().metadata == {"request": "A"}

    await parent()
    assert trace.current_context().metadata == {}
    # parent -> async_child, parent -> sync_child, parent -> read_context.
    # All three are siblings. Four distinct IDs, four starts and four successful
    # completions. Child-local metadata never leaks into the parent or a sibling.


# STREAM-01: Concurrent native BAML streams alongside a nonstreaming BAML call.
async def scenario_concurrent_baml_streams_and_regular_call():
    arrivals = 0
    all_entered = asyncio.Event()

    async def consume(label):
        @trace.instrument(
            trace.context(metadata={"branch": label}), name="consume_stream"
        )
        async def body():
            nonlocal arrivals
            arrivals += 1
            if arrivals == 4:
                all_entered.set()
            await all_entered.wait()
            expected = {"request": "A", "branch": label}
            stream = await baml.stream_text_stream_async(
                label, _baml={"trace": trace.context(metadata={"operation": "stream"})}
            )
            assert trace.current_context().metadata == expected
            chunks = []
            async for chunk in stream:
                assert chunk and label.startswith(chunk)
                assert trace.current_context().metadata == expected
                chunks.append(chunk)
                await asyncio.sleep(0)
            assert chunks
            assert await stream.final_async() == label
            assert trace.current_context().metadata == expected
            # Check a BAML sibling after draining: stream-specific context must
            # not leak out of the opening, next, or final accessor calls.
            assert (await baml.read_context_async()).metadata == expected
            return label

        return await body()

    @trace.instrument(trace.context(metadata={"branch": "regular"}), name="regular")
    async def regular():
        nonlocal arrivals
        arrivals += 1
        if arrivals == 4:
            all_entered.set()
        await all_entered.wait()
        assert (
            await baml.echo_async(
                7, _baml={"trace": trace.context(metadata={"operation": "regular"})}
            )
            == 7
        )
        expected = {"request": "A", "branch": "regular"}
        assert trace.current_context().metadata == expected
        assert (await baml.read_context_async()).metadata == expected
        return 7

    @trace.instrument(trace.context(metadata={"request": "A"}), name="request")
    async def request():
        results = await asyncio.gather(
            consume("hello"), consume("goodbye"), consume("count"), regular()
        )
        assert trace.current_context().metadata == {"request": "A"}
        return results

    assert await request() == ["hello", "goodbye", "count", 7]
    assert trace.current_context().metadata == {}
    # Four host siblings beneath request. Each stream and its accessor operations
    # must remain associated with its own opening invocation/branch; polling must
    # never select another stream's identity or context. The regular echo belongs
    # to regular. Every actual invocation completes exactly once, independently
    # of chunk count; final_async after exhaustion cannot duplicate completion.
    # Stream-opening lifetime versus accessor spans follows the native BAML stream
    # contract; do not apply the optional host-generator lifetime rules here.


# CANCEL-01: Arrange a non-eager task cancelled before its first execution.
async def scenario_cancel_before_entry():
    @trace.instrument
    async def work():
        raise AssertionError("Must not enter")

    task = asyncio.create_task(work())
    task.cancel()
    try:
        await task
    except asyncio.CancelledError:
        pass
    else:
        raise AssertionError("Expected cancellation")
    # No host invocation exists if body never entered. Eager entry is a different case.


# CANCEL-02: Span ends when cancellation cleanup actually exits.
async def scenario_cancel_during_cleanup():
    entered, cleaning, release = asyncio.Event(), asyncio.Event(), asyncio.Event()

    @trace.instrument(trace.context(metadata={"stage": "work"}), name="work")
    async def work():
        entered.set()
        try:
            await asyncio.Event().wait()
        finally:
            cleaning.set()
            await release.wait()
            assert (await baml.read_context_async()).metadata == {"stage": "work"}

    task = asyncio.create_task(work())
    await entered.wait()
    task.cancel()
    await cleaning.wait()
    assert not task.done()
    # No terminal record yet. Cleanup BAML call is work's child with fixed context.
    release.set()
    try:
        await task
    except asyncio.CancelledError:
        pass
    else:
        raise AssertionError("Expected cancellation")
    # One cancelled completion. Original caller context restored.
    assert trace.current_context().metadata == {}


# CANCEL-03: Handled cancellation is success; names do not classify cancellation.
async def scenario_handled_cancellation():
    entered = asyncio.Event()

    @trace.instrument(name="handled")
    async def handled():
        entered.set()
        try:
            await asyncio.Event().wait()
        except asyncio.CancelledError:
            return 7

    task = asyncio.create_task(handled())
    await entered.wait()
    task.cancel()
    assert await task == 7
    # handled records success.

    class CancelledError(Exception):
        pass

    failure = CancelledError("ordinary application failure")

    @trace.instrument(name="misleading_name")
    async def misleading_name():
        raise failure

    try:
        await misleading_name()
    except CancelledError as caught:
        assert caught is failure
    else:
        raise AssertionError("Expected application failure")
    # misleading_name records error, never cancellation by name.


# CANCEL-04: Parent cancellation does not end its shielded child.
async def scenario_shielded_child():
    entered, release = asyncio.Event(), asyncio.Event()
    children = []

    @trace.instrument(name="child")
    async def child():
        entered.set()
        await release.wait()
        return trace.current_context().metadata

    @trace.instrument(trace.context(metadata={"request": "A"}), name="parent")
    async def parent():
        task = asyncio.create_task(child())
        children.append(task)
        return await asyncio.shield(task)

    task = asyncio.create_task(parent())
    await entered.wait()
    task.cancel()
    try:
        await task
    except asyncio.CancelledError:
        pass
    else:
        raise AssertionError("Expected parent cancellation")
    assert not children[0].done()
    # Parent ended cancelled; child is still active and retains parentage.
    release.set()
    assert await children[0] == {"request": "A"}
    # Child ends success later; parent is never reopened.


# CANCEL-05: Native TaskGroup errors/cancellation stay intact.
async def scenario_taskgroup_failure():
    entered = asyncio.Event()
    failure = ValueError("sibling failed")

    @trace.instrument(name="waiting")
    async def waiting():
        entered.set()
        await asyncio.Event().wait()

    @trace.instrument(name="failing")
    async def failing():
        await entered.wait()
        raise failure

    @trace.instrument(name="parent")
    async def parent():
        try:
            async with asyncio.TaskGroup() as group:
                group.create_task(waiting())
                group.create_task(failing())
        except ExceptionGroup as errors:
            assert errors.exceptions == (failure,)
        else:
            raise AssertionError("Expected TaskGroup failure")

    await parent()
    # parent -> waiting(cancelled), failing(error); parent handles group and succeeds.


# CANCEL-05/CANCEL-06: Timeout and completion/cancellation race.
async def scenario_timeout_and_completion_race():
    @trace.instrument(name="timeout")
    async def blocked():
        await asyncio.Event().wait()

    try:
        await asyncio.wait_for(blocked(), timeout=0.01)
    except TimeoutError:
        pass
    else:
        raise AssertionError("Expected timeout")
    # Timeout semantics preserved; actual body cancellation produces one terminal.

    entered, release = asyncio.Event(), asyncio.Event()

    @trace.instrument(name="race")
    async def race():
        entered.set()
        await release.wait()
        return 7

    task = asyncio.create_task(race())
    await entered.wait()
    release.set()
    task.cancel()
    try:
        assert await task == 7
    except asyncio.CancelledError:
        pass
    # Repeat with either ordering winning. Outcome must match body execution;
    # at most one terminal record, never success plus cancelled for the same ID.


# THREAD-01: asyncio.to_thread carries native context to its worker.
async def scenario_to_thread():
    caller_thread = threading.get_ident()

    @trace.instrument(name="worker")
    def worker():
        assert threading.get_ident() != caller_thread
        assert trace.current_context().metadata == {"request": "A"}
        return baml.read_context()

    @trace.instrument(trace.context(metadata={"request": "A"}), name="parent")
    async def parent():
        return await asyncio.to_thread(worker)

    assert (await parent()).metadata == {"request": "A"}
    # parent -> worker -> read_context. Follow to_thread's actual copy point.


# THREAD-02: Explicit copied versus empty context, independent of thread defaults.
def scenario_explicit_thread_context():
    @trace.instrument(name="worker")
    def worker():
        return baml.read_context()

    @trace.instrument(trace.context(metadata={"request": "A"}), name="parent")
    def parent():
        copied = contextvars.copy_context()
        with ThreadPoolExecutor(max_workers=1) as pool:
            inherited = pool.submit(copied.run, worker).result()
            independent = pool.submit(contextvars.Context().run, worker).result()
        assert inherited.metadata == {"request": "A"}
        assert independent.metadata == {}

    parent()
    # Copied worker is parent's child; empty-context worker is an independent root.
    # Raw threads/run_in_executor follow explicit host context rules, not global state.


# THREAD-08: Ordinary pool workers reuse threads without reusing job context.
def scenario_reused_thread_pool_workers():
    assert trace.current_context().metadata == {}

    @trace.instrument(name="nested")
    def nested():
        return baml.read_context()

    def run(job):
        assert trace.current_context().metadata == {}

        @trace.instrument(trace.context(metadata={"job": job}), name="job")
        def root():
            expected = {"job": job}
            assert nested().metadata == expected
            assert trace.current_context().metadata == expected
            return threading.get_ident(), job

        result = root()
        assert trace.current_context().metadata == {}
        return result

    # Start the pool with the caller's tracing context empty. Submit ordinary
    # application callables directly; no bind or copy_context propagation.
    with ThreadPoolExecutor(max_workers=1) as pool:
        results = [pool.submit(run, job).result() for job in range(10)]
    assert len({thread_id for thread_id, _ in results}) == 1
    assert [job for _, job in results] == list(range(10))
    assert trace.current_context().metadata == {}
    # Same worker thread, ten independent job roots. Each job -> nested ->
    # read_context. Thirty distinct IDs, thirty starts/successful completions.
    # No invocation inherits metadata or parentage from a previous job.


# THREAD-03: Proposed bind captures at handoff; it starts no invocation itself.
def scenario_bound_thread_handoff():
    @trace.instrument(name="child")
    def child():
        return baml.read_context()

    @trace.instrument(trace.context(metadata={"request": "A"}), name="parent")
    def parent():
        return trace.bind(child)

    @trace.instrument(trace.context(metadata={"request": "worker"}), name="worker")
    def worker(bound):
        result = bound()
        assert trace.current_context().metadata == {"request": "worker"}
        return result

    bound = parent()  # parent ends before the worker executes.
    with ThreadPoolExecutor(max_workers=1) as pool:
        assert pool.submit(worker, bound).result().metadata == {"request": "A"}
        assert pool.submit(trace.current_context).result().metadata == {}
    # parent -> child -> read_context; worker is another root. Restore worker frame.


# THREAD-04: Proposed concurrent bind reuse and failure restoration.
def scenario_bound_callable_reuse():
    barrier = threading.Barrier(2)
    failure = ValueError("worker failure")

    @trace.instrument(name="child")
    def child(value):
        barrier.wait()
        assert trace.current_context().metadata == {"request": "A"}
        if value == 2:
            raise failure
        return baml.echo(value)

    @trace.instrument(trace.context(metadata={"request": "A"}), name="parent")
    def parent():
        return trace.bind(child)

    bound = parent()
    with ThreadPoolExecutor(max_workers=2) as pool:
        first = pool.submit(bound, 1)
        second = pool.submit(bound, 2)
        assert first.result() == 1
        try:
            second.result()
        except ValueError as caught:
            assert caught is failure
        else:
            raise AssertionError("Expected worker failure")
        assert pool.submit(trace.current_context).result().metadata == {}
        assert pool.submit(trace.current_context).result().metadata == {}
    # Two distinct child IDs under captured parent; success/error, restored workers.
    # bind must not concurrently enter one mutable Context object.
    # Decide separately whether bind carries tracing only or all application context.


# THREAD-05: Proposed async bind installs captured state during execution.
async def scenario_async_bind():
    @trace.instrument(name="child")
    async def child():
        await asyncio.sleep(0)
        return trace.current_context()

    @trace.instrument(trace.context(metadata={"request": "A"}), name="parent")
    def parent():
        return trace.bind(child)

    @trace.instrument(trace.context(metadata={"request": "B"}), name="consume")
    async def consume(bound):
        coroutine = bound()
        assert trace.current_context().metadata == {"request": "B"}
        assert (await coroutine).metadata == {"request": "A"}
        assert trace.current_context().metadata == {"request": "B"}

    await consume(parent())
    # parent -> child. consume remains separate. Creation-only binding is insufficient.


# THREAD-06: Cancelling an await cannot terminate an actual running thread body.
async def scenario_cancelled_thread_awaiter():
    loop = asyncio.get_running_loop()
    entered, finished = asyncio.Event(), asyncio.Event()
    release = threading.Event()

    @trace.instrument(name="worker")
    def worker():
        loop.call_soon_threadsafe(entered.set)
        release.wait()
        return 7

    def run_worker():
        try:
            assert worker() == 7
        finally:
            loop.call_soon_threadsafe(finished.set)

    task = asyncio.create_task(asyncio.to_thread(run_worker))
    await entered.wait()
    task.cancel()
    try:
        await task
    except asyncio.CancelledError:
        pass
    else:
        raise AssertionError("Expected awaiter cancellation")
    # Worker still active. No cancelled terminal record for its still-running body.
    release.set()
    await finished.wait()
    # Worker ends successfully exactly once, even though its awaiter was cancelled.


# THREAD-07: Proposed support for independent application loops on two threads.
def scenario_multiple_application_loops():
    barrier = threading.Barrier(2)

    def application_thread(request_id):
        async def main():
            @trace.instrument(name="callback")
            async def callback(value):
                assert asyncio.get_running_loop() is loop
                assert trace.current_context().metadata == {"request": request_id}
                return await baml.echo_async(value)

            @trace.instrument(
                trace.context(metadata={"request": request_id}), name="request"
            )
            async def request():
                return await baml.call_async(callback, 7)

            loop = asyncio.get_running_loop()
            barrier.wait()
            assert await request() == 7

        asyncio.run(main())

    with ThreadPoolExecutor(max_workers=2) as pool:
        first = pool.submit(application_thread, "A")
        second = pool.submit(application_thread, "B")
        first.result()
        second.result()
    # Separate roots and originating loops. Most-recent/global loop selection fails.


# CB-01: An unmarked dispatched callback is still a host invocation.
def scenario_plain_callback():
    def callback(value):
        assert trace.current_context().metadata == {"request": "A"}
        return baml.echo(value + 1)

    @trace.instrument(trace.context(metadata={"request": "A"}), name="request")
    def request():
        return baml.call(callback, 6)

    assert request() == 7
    # request -> BAML call -> host callback -> BAML echo.
    # Callback registration itself creates nothing and captures no invocation parent.


# CB-02: Marked async callback adopts dispatch exactly once.
async def scenario_marked_callback():
    @trace.instrument(name="lookup")
    async def callback(value):
        return await baml.echo_async(value + 1)

    @trace.instrument(name="request")
    async def request():
        return await baml.call_async(callback, 6)

    assert await request() == 7
    # request -> call -> lookup -> echo. One lookup, not dispatch plus wrapper.


# CB-03: Callback merges inherited, marker, then explicit BAML call configuration.
async def scenario_callback_context_precedence():
    @trace.instrument(
        trace.context(
            distinct_id="marker-user",
            metadata={"stage": "marker", "marker": True, "remove": "marker"},
        ),
        name="callback",
    )
    async def callback(value):
        before = trace.current_context()
        assert before.distinct_id == "call-user"
        assert before.metadata == {"root": True, "marker": True, "stage": "dispatch"}
        child = await baml.read_context_async(
            _baml={"trace": trace.context(metadata={"stage": "reentry"})}
        )
        assert child.distinct_id == "call-user"
        assert child.metadata == {"root": True, "marker": True, "stage": "reentry"}
        assert trace.current_context().metadata == before.metadata
        return value

    @trace.instrument(
        trace.context(distinct_id="root-user", metadata={"root": True}), name="request"
    )
    async def request():
        result = await baml.call_then_context_async(
            callback,
            7,
            trace.context(
                distinct_id="call-user", metadata={"stage": "dispatch", "remove": None}
            ),
            _baml={"trace": trace.context(metadata={"stage": "baml"})},
        )
        assert result.distinct_id == "root-user"
        assert result.metadata == {"root": True, "stage": "baml"}
        assert trace.current_context().metadata == {"root": True}

    await request()
    # request -> call_then_context -> callback -> read_context.
    # Adopted wrapper never reapplies marker stage over dispatch stage.


# CB-04: Recursive invocation must not reuse the dispatched entry's adoption.
async def scenario_callback_recursion():
    @trace.instrument(trace.context(metadata={"stage": "marker"}), name="lookup")
    async def lookup(value):
        if value == 1:
            assert trace.current_context().metadata == {"stage": "dispatch"}
            return await lookup(0)
        assert trace.current_context().metadata == {"stage": "marker"}
        return await baml.echo_async(7)

    assert (
        await baml.call_configured_async(
            lookup, 1, trace.context(metadata={"stage": "dispatch"})
        )
        == 7
    )
    # call_configured -> lookup(dispatch) -> lookup(marker) -> echo. Distinct IDs.


# CB-05: Dispatch adopts only the outermost recognized wrapper.
def scenario_stacked_callback():
    @trace.instrument(trace.context(metadata={"stage": "outer"}), name="outer")
    @trace.instrument(trace.context(metadata={"stage": "inner"}), name="inner")
    def callback(value):
        assert trace.current_context().metadata == {"stage": "inner"}
        assert baml.read_context().metadata == {"stage": "inner"}
        return value

    assert (
        baml.call_configured(callback, 7, trace.context(metadata={"stage": "dispatch"}))
        == 7
    )
    # call_configured -> outer(dispatch) -> inner(inner) -> read_context.
    # Two host invocations, never three. Outer start/end context stays dispatch.


# CB-06: Same callback overlaps, is dispatched again, and is later called directly.
async def scenario_callback_reuse():
    arrivals = 0
    both_entered = asyncio.Event()

    @trace.instrument(name="callback")
    async def callback(value):
        nonlocal arrivals
        if value in (1, 2):
            arrivals += 1
            if arrivals == 2:
                both_entered.set()
            await both_entered.wait()
            assert trace.current_context().metadata == {"request": value}
        else:
            assert trace.current_context().metadata == {}
        return await baml.echo_async(value)

    results = await asyncio.gather(
        baml.call_async(
            callback, 1, _baml={"trace": trace.context(metadata={"request": 1})}
        ),
        baml.call_async(
            callback, 2, _baml={"trace": trace.context(metadata={"request": 2})}
        ),
    )
    assert results == [1, 2]
    assert await baml.call_async(callback, 3) == 3
    assert await callback(4) == 4
    # Each dispatch has its own token/identity. Later direct call is a fresh root.
    # Context must come from dispatch, never freeze callback registration context.


# CB-07: Explicit mode wins; context-only options keep marker default.
def scenario_callback_mode_precedence():
    @trace.instrument(
        trace.timing().span(inputs=True).mode(trace.Mode.Timing), name="callback"
    )
    def callback(value):
        return baml.echo(value)

    assert (
        baml.call_configured(callback, 1, trace.context(metadata={"dispatch": True}))
        == 1
    )
    assert baml.call_configured(callback, 2, trace.hidden()) == 2
    assert baml.call_configured(callback, 3, trace.span(inputs=False)) == 3
    # First callback stays Timing; second Hidden but echo still visible; third Span.
    # Under draft request semantics, false doesn't veto marker's inputs=True request.
    # Repeat with recording disabled: mode/context resolution still applies.


# CB-08: Async callback can use originating loop resources and application context.
async def scenario_callback_application_environment():
    loop = asyncio.get_running_loop()
    application_context = contextvars.ContextVar("application", default="missing")
    application_context.set("request A")
    reply = loop.create_future()

    @trace.instrument
    async def callback(value):
        assert asyncio.get_running_loop() is loop
        assert application_context.get() == "request A"
        loop.call_soon(reply.set_result, value)
        return await reply

    assert await baml.call_async(callback, 7) == 7
    assert application_context.get() == "request A"
    # Callback may be another task; a fresh private event loop cannot satisfy this.


# CB-09: Proposed execution cells beyond sync->sync and async->async above.
async def scenario_async_entry_sync_callback():
    @trace.instrument
    def callback(value):
        return baml.echo(value + 1)

    assert await baml.call_async(callback, 6) == 7
    # Support deliberately or reject clearly; never block the servicing event loop.


def scenario_sync_entry_async_callback():
    @trace.instrument
    async def callback(value):
        return await baml.echo_async(value + 1)

    assert baml.call(callback, 6) == 7
    # Proposed cell: needs its own loop/scheduling contract. Also decide sync BAML
    # entry from a thread that already runs an application loop. Do not claim all cells.


# CB-10: Original Python error survives host->BAML->host round-trip.
async def scenario_callback_error():
    failure = ValueError("application failure")

    @trace.instrument(trace.context(metadata={"stage": "callback"}), name="callback")
    async def callback(value):
        raise failure

    @trace.instrument(trace.context(metadata={"stage": "parent"}), name="parent")
    async def parent():
        try:
            await baml.call_async(callback, 1)
        except ValueError as caught:
            assert caught is failure
        else:
            raise AssertionError("Expected callback failure")
        return await baml.read_context_async()

    assert (await parent()).metadata == {"stage": "parent"}
    # Callback and call record error. Parent catches and succeeds; later leaf is sibling.


# CB-11: Proposed cell with actual cancellation delivered to the callback.
async def scenario_callback_cancellation():
    entered, cleaning, release, exited = (asyncio.Event() for _ in range(4))

    @trace.instrument(trace.context(metadata={"stage": "callback"}), name="callback")
    async def callback(value):
        entered.set()
        try:
            await asyncio.Event().wait()
        finally:
            cleaning.set()
            await release.wait()
            assert trace.current_context().metadata == {"stage": "callback"}
            exited.set()

    task = asyncio.create_task(baml.call_async(callback, 1))
    await entered.wait()
    task.cancel()
    await cleaning.wait()
    # No callback terminal record while cleanup waits, even if BAML caller has ended.
    release.set()
    try:
        await task
    except asyncio.CancelledError:
        pass
    else:
        raise AssertionError("Expected BAML call cancellation")
    await exited.wait()
    # Callback records cancellation only when its own execution exits, once.
    # If the selected bridge cell doesn't deliver cancellation, reject/describe it;
    # a callback that continues and returns must record its actual outcome instead.


# CB-12: An unknown wrapper is not heuristically merged by name.
def scenario_third_party_callback_wrapper():
    def application_wrapper(fn):
        @functools.wraps(fn)
        def wrapped(value):
            return fn(value)

        return wrapped

    @application_wrapper
    @trace.instrument(name="callback")
    def callback(value):
        return value

    assert baml.call(callback, 7) == 7
    # Correct execution. Automatic adoption is promised only for recognized SDK
    # wrappers; unknown stacks may add a boundary. Never suppress real recursion.


# ID-01: Reservation freezes identity/config, not inherited context or parent.
def scenario_reserved_baml_call():
    reserved = trace.span(output=True).context(metadata={"stage": "extract"}).reserve()
    identity = reserved.id()

    @trace.instrument(trace.context(metadata={"request": "later"}), name="parent")
    def parent():
        result = baml.read_context(_baml={"trace": reserved})
        assert result.metadata == {"request": "later", "stage": "extract"}

    parent()
    assert reserved.id() == identity
    assert reserved.inspect().mode == trace.Mode.Span
    assert reserved.inspect().context.metadata == {"stage": "extract"}
    # parent -> read_context with reserved identity. No extra reservation span.


# ID-02: Single-use attachment through an alias, including concurrent attempts.
def scenario_reservation_alias():
    reserved = trace.span().reserve()
    alias = reserved
    assert baml.echo(1, _baml={"trace": reserved}) == 1
    try:
        baml.echo(2, _baml={"trace": alias})
    except Exception:
        pass  # Decide the generated attachment-error type; application ValueError is unrelated.
    else:
        raise AssertionError("Reservation must not attach twice")
    assert alias.id() == reserved.id()
    # Only one echo invocation entered. Failed attachment creates no second echo.


def scenario_concurrent_reservation_attachment():
    reserved = trace.span().reserve()
    barrier = threading.Barrier(4)

    def attempt(value):
        barrier.wait()
        try:
            return True, baml.echo(value, _baml={"trace": reserved})
        except Exception:
            return False, value  # Expected documented attachment rejection.

    with ThreadPoolExecutor(max_workers=4) as pool:
        futures = [pool.submit(attempt, value) for value in range(4)]
        results = [future.result() for future in futures]
    assert sum(succeeded for succeeded, _ in results) == 1
    # Exactly one actual BAML target invocation, three attachment failures; stable ID.


# ID-03: Full opaque value equality survives a BAML round-trip.
def scenario_span_id_equality():
    first = trace.span().reserve().id()
    second = trace.span().reserve().id()
    assert baml.identity_span_id(first) == first
    assert first != second
    # Equality includes recording scope plus local identity, never only object 'is'.
    # No public constructor/components/serialization. A wrong-runtime attachment
    # must fail without consuming a reservation usable by its original runtime.


# ID-04: Host marker rejects reservations; BAML-managed callbacks may adopt them.
def scenario_host_and_callback_reservations():
    reserved = trace.span().reserve()
    try:
        trace.instrument(reserved)
    except trace.TraceUsageError:
        pass
    else:
        raise AssertionError("Host marker accepts reusable Options only")
    assert (
        baml.echo(7, _baml={"trace": reserved}) == 7
    )  # Rejection must not consume it.

    @trace.instrument(trace.timing(), name="callback")
    def callback(value):
        return value

    identity = baml.reserve_callback(callback)
    assert isinstance(identity, trace.SpanId)
    # One callback Span with BAML-assigned reserved ID, despite Timing marker.
    # No host reservation factory or ambient current_span_id getter is added.


# CAP-01: Capture argument names/defaults; omit implicit receivers.
def scenario_named_input_capture():
    default = [2]

    @trace.instrument(trace.span(inputs=True, output=True), name="work")
    def work(first, /, second=default, *rest, flag=True, **kwargs):
        return first + second[0]

    assert work(1, flag=False, extra="x") == 3
    # Recorded inputs: first=1, second=[2], rest=(), flag=False, kwargs={extra: 'x'}.
    # Decide serialized tuple/list spelling. Capture output=3, not a live reference.

    class Service:
        @trace.instrument(trace.span(inputs=True), name="method")
        def method(self, value=7):
            return value

    assert Service().method() == 7
    # method input contains value=7, no self. Do not re-evaluate defaults/annotations.
    # A standalone function parameter merely called self is not implicitly a receiver.


# CAP-02: Inputs/output freeze at their observation points.
def scenario_capture_snapshots():
    value = {"items": [1]}

    @trace.instrument(trace.span(inputs=True, output=True), name="work")
    def work(value):
        value["items"].append(2)
        return value

    result = work(value)
    assert result is value
    result["items"].append(3)
    assert value == {"items": [1, 2, 3]}
    # Recorded input [1], output [1, 2]. Later mutation cannot change either capture.


# CAP-03: Capture must not execute arbitrary discovery hooks or consume iterators.
def scenario_hostile_capture_values():
    hooks_called = []

    class Opaque:
        def __repr__(self):
            hooks_called.append("repr")
            raise AssertionError("Do not invoke repr")

        @property
        def value(self):
            hooks_called.append("getter")
            raise AssertionError("Do not invoke getter")

        def __iter__(self):
            hooks_called.append("iterator")
            raise AssertionError("Do not invoke iterator")

        def toJSON(self):
            hooks_called.append("serializer")
            raise AssertionError("Do not invoke serializer")

    @trace.instrument(trace.span(inputs=True, output=True), name="opaque")
    def opaque(value):
        return value

    value = Opaque()
    assert opaque(value) is value
    assert hooks_called == []
    # Unsupported payload is explicitly unavailable; body still succeeds.

    @trace.instrument(trace.span(inputs=True), name="iterator")
    def consume(iterator):
        return next(iterator)

    iterator = iter([1, 2])
    assert consume(iterator) == 1
    assert next(iterator) == 2
    # Input encoder did not consume it. Known SDK encoders must avoid hostile overrides.


# CAP-04: Bounded traversal, cycles versus shared references, and byte accounting.
def scenario_bounded_capture():
    cyclic = {"safe": 7}
    cyclic["cycle"] = cyclic
    shared = [1]
    repeated = {"left": shared, "right": shared}

    @trace.instrument(trace.span(inputs=True, output=True), name="work")
    def work(value):
        return value

    for value in (
        cyclic,
        repeated,
        [[[[[[[1]]]]]]],
        list(range(100_000)),
        "🙂" * 100_000,
    ):
        assert work(value) is value
    # Each succeeds without unbounded traversal. Preserve representable siblings.
    # Mark cycles/depth/item/byte truncation with reasons; repeated references aren't cycles.
    # Choose concrete limits; byte budget counts UTF-8 bytes, not characters.


# CAP-05: Outcomes don't depend on payload capture or the exception display name.
def scenario_capture_flags():
    @trace.instrument(trace.span(inputs=False, output=False, error=False), name="work")
    def work(value):
        return value

    assert work(7) == 7
    # With no other policy, no payloads; still name/identity/timing/success outcome.
    # false adds no request; another applicable policy's true request may still capture.
    # Retained Timing invocations use the same configured capture-policy contract.


# ERR-01: Encoding failure must not replace original application errors.
def scenario_error_capture_failure():
    class Failure(Exception):
        def __str__(self):
            raise AssertionError(
                "Formatting failure must not replace application error"
            )

    failure = Failure()

    @trace.instrument(trace.span(inputs=True, error=True), name="child")
    def child(value):
        raise failure

    @trace.instrument(name="parent")
    def parent():
        try:
            child(object())
        except Failure as caught:
            assert caught is failure
        else:
            raise AssertionError("Expected application failure")
        return baml.echo(7)

    assert parent() == 7
    # parent -> child(error), parent -> echo; parent success. Unavailable input/error
    # details don't erase outcome or corrupt restoration.


# ERR-01: Restore the frame on BaseException too.
def scenario_baseexception_exit():
    class ApplicationAbort(BaseException):
        pass

    failure = ApplicationAbort()

    @trace.instrument(trace.context(metadata={"stage": "work"}))
    def work():
        raise failure

    try:
        work()
    except ApplicationAbort as caught:
        assert caught is failure
    else:
        raise AssertionError("Expected application abort")
    assert trace.current_context().metadata == {}
    # Error outcome. Merely catching Exception in tracing would miss this path.


# ERR-02: Run under operational failure at each recording/capture/delivery stage.
def scenario_recording_failure_containment():
    result = object()
    failure = ValueError("application failure")

    @trace.instrument(
        trace.span(inputs=True, output=True, error=True).context(
            metadata={"stage": "body"}
        )
    )
    def work(should_fail):
        assert trace.current_context().metadata == {"stage": "body"}
        if should_fail:
            raise failure
        return result

    assert work(False) is result
    try:
        work(True)
    except ValueError as caught:
        assert caught is failure
    else:
        raise AssertionError("Expected application failure")
    assert trace.current_context().metadata == {}
    # Repeat with input encoding, start publication, output/error encoding, terminal
    # publication, or destination delivery failing. Bounded diagnostics; no body retry,
    # replaced result/error, stale frame, or invented successful delivery.


# ERR-03: Invalid marker configuration is an application-facing usage error.
def scenario_invalid_marker_configuration():
    for invalid in (trace.current_context(), {"mode": "Span"}, 42, "span"):
        try:
            trace.instrument(invalid)
        except trace.TraceUsageError:
            pass
        else:
            raise AssertionError("Invalid marker configuration must be rejected")
    # No body/invocation/ambient change. A Context snapshot isn't reusable Options.


# ERR-03/ERR-04: Primitive metadata only; bounds/finite-number policy is proposed.
def scenario_metadata_validation():
    primitives = {"string": "x", "int": 7, "float": 1.5, "bool": True}
    assert trace.context(metadata=primitives).inspect().context.metadata == primitives

    for invalid in (
        {"nested": {}},
        {"nested": []},
        {"object": object()},
        {7: "bad key"},
    ):
        try:
            trace.context(metadata=invalid)
        except (TypeError, ValueError):
            pass  # Settle exact builder error classes consistently with BEP-077.
        else:
            raise AssertionError("Expected invalid metadata rejection")
    # Decide NaN/infinity, metadata key/value/total budgets, distinct_id bounds, and
    # rejection versus truncation. Bodies and records must agree on effective context.


# API-06: Initial native-generator rejection, if generator support is deferred.
def scenario_unsupported_generator_shapes():
    def generator():
        yield 1

    async def async_generator():
        yield 1

    for fn in (generator, async_generator):
        try:
            trace.instrument(fn)
        except trace.TraceUsageError:
            pass
        else:
            raise AssertionError(
                "Deferred generator form must not silently trace creation"
            )
    # Callable objects/custom awaitables/descriptors need equally explicit support rules.


# CCT-01: Repeated paths aggregate, while different caller paths stay separate.
def scenario_timing_call_paths():
    @trace.instrument(trace.timing(), name="load")
    def load(value):
        return value

    @trace.instrument(trace.timing(), name="refresh")
    def refresh():
        for value in range(3):
            load(value)

    @trace.instrument(trace.timing(), name="search")
    def search():
        for value in range(2):
            load(value)

    @trace.instrument(name="request")
    def request():
        refresh()
        search()

    request()
    # load under refresh: 3 calls; under search: 2. Same definition, distinct paths.
    # Keys: program, parent path, definition, call site, call/spawn edge; never metadata.


# CCT-02: Fresh function objects and metadata don't invent structural definitions.
def scenario_recreated_local_functions():
    @trace.instrument(trace.timing(), name="request")
    def request():
        for attempt in range(4):

            @trace.instrument(
                trace.timing().context(metadata={"attempt": attempt}), name="local"
            )
            def work():
                return attempt

            assert work() == attempt

    request()
    # One lexical definition/call path with 4 calls, despite four function objects.
    # Unknown source/call-site uses an absent value; don't fabricate a location.


def scenario_same_display_name_different_definitions():
    @trace.instrument(trace.timing(), name="same")
    def first():
        return 1

    @trace.instrument(trace.timing(), name="same")
    def second():
        return 2

    assert first() == 1
    assert second() == 2
    # Distinct definitions/path buckets. Display labels aren't structural identity.
    # Explicit stacked wrappers likewise preserve separate structural boundaries.


# CCT-03: Timing outcomes remain meaningful with no individually retained spans.
async def scenario_timing_outcomes():
    entered = asyncio.Event()

    @trace.instrument(trace.timing(), name="work")
    async def work(mode):
        if mode == "error":
            raise ValueError("failure")
        if mode == "cancel":
            entered.set()
            await asyncio.Event().wait()
        return 7

    for mode in ("ok", "error", "ok", "cancel"):
        task = asyncio.create_task(work(mode))
        if mode == "cancel":
            await entered.wait()
            task.cancel()
        try:
            assert await task == 7
        except ValueError:
            assert mode == "error"
        except asyncio.CancelledError:
            assert mode == "cancel"
    # Same launch path: 4 calls, 2 success, 1 error, 1 cancelled; await duration included.
    # Late retention cannot count outcomes twice; Hidden omits own timing, not children.


# PROC-01/PROC-02: Real application process targets must be module-level for spawn.
# These worker functions are application code needed by the examples, not a harness.
def process_job(value):
    assert trace.current_context().metadata == {}

    @trace.instrument(trace.context(metadata={"job": value}), name="child_process")
    def work():
        assert baml.read_context().metadata == {"job": value}
        return baml.echo(value)

    result = work()
    assert trace.current_context().metadata == {}
    return result


def scenario_spawned_process_pool():
    @trace.instrument(trace.context(metadata={"request": "parent"}), name="parent")
    def parent():
        with multiprocessing.get_context("spawn").Pool(processes=1) as pool:
            assert pool.map(process_job, [1, 2]) == [1, 2]
        assert trace.current_context().metadata == {"request": "parent"}

    parent()
    # Proposed isolation: worker has its fresh runtime/recording; jobs have independent
    # roots and no leftover metadata. Parent isn't linked across process by memory copy.
    # Repeat with forkserver where supported. Don't change global start method.


# PROC-03: Proposed policy rejects inherited runtime use after fork.
def process_using_inherited_runtime():
    try:
        baml.echo(7)
    except RuntimeError:
        return  # Replace with the documented inherited-runtime error once decided.
    raise AssertionError(
        "Inherited runtime must be rejected before touching its workers"
    )


def scenario_fork_after_runtime_started():
    assert baml.echo(1) == 1
    process = multiprocessing.get_context("fork").Process(
        target=process_using_inherited_runtime
    )
    process.start()
    process.join()
    assert process.exitcode == 0
    assert baml.echo(2) == 2
    # Platform-dependent/proposed. Child must fail promptly, not hang on inherited locks
    # or corrupt parent's recorder. Safe reinitialization is an alternative to design.


# PROC-04: Child failure doesn't choose parent's outcome.
def failing_process_job():
    @trace.instrument(name="child_process")
    def work():
        raise ValueError("child failure")

    work()


def scenario_child_process_failure():
    @trace.instrument(name="parent_process")
    def parent():
        process = multiprocessing.get_context("spawn").Process(
            target=failing_process_job
        )
        process.start()
        process.join()
        assert process.exitcode != 0
        return baml.echo(7)

    assert parent() == 7
    # Child error in child's recording; parent handles result and succeeds.
    # Abruptly killed processes can have incomplete spans; never invent success.
    # No new durability/shutdown/flush promise.


# PROC-05/PROC-06: Deferred portability/transport contract.
# Sending a reservation/runtime handle/frame/bound closure must not transfer live
# authority by accident. Decide explicit serialization rejection/portable config.
# Cross-process connected ancestry requires its own validated propagation envelope,
# remote parent identity, scopes, clocks, and loss contract. No such API added here.


# GEN-01/GEN-02: Optional generators start at first resume and restore consumer.
def scenario_generator_first_execution():
    @trace.instrument(
        trace.span(inputs=True).context(metadata={"stage": "producer"}), name="items"
    )
    def items(values):
        for value in values:
            assert trace.current_context().metadata["stage"] == "producer"
            yield baml.echo(value)

    values = [1]
    iterator = items(values)  # No invocation or input capture here.
    values.append(2)

    @trace.instrument(trace.context(metadata={"stage": "consumer"}), name="consume")
    def consume():
        assert next(iterator) == 1
        assert trace.current_context().metadata == {"stage": "consumer"}
        assert baml.read_context().metadata == {"stage": "consumer"}
        assert next(iterator) == 2
        iterator.close()

    consume()
    # consume -> items -> echoes; between-yield read_context belongs to consume.
    # Captured input [1, 2]. One generator lifetime includes time between yields.
    unused = items(values)
    unused.close()
    # Never-started generator creates no invocation.


# GEN-03: A later consumer can't reparent a started generator.
def scenario_generator_different_consumers():
    @trace.instrument(name="items")
    def items():
        yield baml.read_context()
        yield baml.read_context()

    iterator = items()

    @trace.instrument(trace.context(metadata={"request": "A"}), name="first")
    def first():
        return next(iterator)

    @trace.instrument(trace.context(metadata={"request": "B"}), name="second")
    def second():
        result = next(iterator)
        assert trace.current_context().metadata == {"request": "B"}
        return result

    assert first().metadata == {"request": "A"}
    assert second().metadata == {"request": "A"}
    iterator.close()
    # first -> items remains ancestry; second isn't items' new parent.


# GEN-04: Optional generator protocol forwarding and terminal-value capture.
def scenario_generator_protocol():
    @trace.instrument(trace.span(output=True, error=True), name="dialogue")
    def dialogue() -> Generator[str, str, str]:
        received = yield "ready"
        try:
            yield received
        except ValueError:
            yield "handled"
        return "terminal"

    iterator = dialogue()
    assert next(iterator) == "ready"
    assert iterator.send("hello") == "hello"
    assert iterator.throw(ValueError("handled inside")) == "handled"
    try:
        next(iterator)
    except StopIteration as finished:
        assert finished.value == "terminal"
    else:
        raise AssertionError("Expected exhaustion")
    iterator.close()
    iterator.close()
    # Handled injected error doesn't end the span. Exhaustion captures 'terminal',
    # never accumulated yielded values. Repeated close produces no duplicate end.
    # Decide early-close outcome; GeneratorExit isn't blindly an application error.


# GEN-05: Optional async-generator resumes/yields restore consumer context.
async def scenario_async_generator():
    @trace.instrument(trace.context(metadata={"stage": "producer"}), name="stream")
    async def stream() -> AsyncGenerator[int, None]:
        yield await baml.echo_async(1)
        yield await baml.echo_async(2)

    @trace.instrument(trace.context(metadata={"stage": "consumer"}), name="consume")
    async def consume():
        values = []
        async for value in stream():
            assert trace.current_context().metadata == {"stage": "consumer"}
            values.append(await baml.echo_async(value))
        return values

    assert await consume() == [1, 2]
    # Fetching echoes belong to stream; processing echoes to consume. No prefetch.
    # Forward anext/asend/athrow/aclose; async generator has no terminal output value.
    # Cancellation ends only when actual generator execution/cleanup exits.


# GEN-06: Abandonment isn't successful exhaustion or a deterministic close.
def scenario_generator_abandonment():
    @trace.instrument(name="items")
    def items():
        yield 1
        yield 2

    @trace.instrument(name="consumer")
    def consumer():
        iterator = items()
        assert next(iterator) == 1
        return iterator

    iterator = consumer()
    # Consumer ended, generator remains suspended/live, with no successful completion.
    iterator.close()
    # Explicit close ends once. No finalization guarantees for an abandoned iterator;
    # tracing must not consume it, force exhaustion, or reopen its parent.


# Python examples above need analogous Node behavior, plus these language-specific cases:
# - instrument(fn, config?), instrument(options, fn, config?), standard method decorators.
#   Preserve this, optional/rest arguments, concrete result type; no legacy-decorator promise.
# - Ordinary function returning native Promise: body runs synchronously; lifetime ends on
#   fulfillment/rejection. Preserve actual result/rejection, including primitive rejection.
# - AsyncLocalStorage: interleaved requests, Promise.all siblings, timers, detached work,
#   native callback application context, and restoration to the caller after entry scope.
# - Sync BAML combinations that starve callback servicing must reject explicitly, never hang,
#   including callbacks reached indirectly through existing handles/closures.
# - Capture actual positional arguments excluding this; don't re-evaluate defaults or invoke
#   getters/toJSON/proxy discovery/iterators. Error named AbortError isn't cancellation proof.
# - Choose SpanId value-equality spelling; reference equality cannot express opaque identity.
# - Worker isolates/child processes need separate runtime state; connected propagation deferred.
# These comments specify Node obligations; Python execution cannot establish them.
