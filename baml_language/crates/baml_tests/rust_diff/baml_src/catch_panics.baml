// Ordinary catch wildcards must exclude panics on every dispatch path. Keep
// these probes separate so a failure identifies the corpus regression.
function CatchPanicDivide(n: int) -> int throws string {
    if (n > 100) { throw "ordinary"; }
    10 / n
}

function CatchPanicIndex(n: int) -> int throws string {
    if (n > 100) { throw "ordinary"; }
    let xs = [1];
    xs[n]
}

function CatchWildcardDivision() -> void {
    let result = CatchPanicDivide(0) catch (e) {
        _ => -1,
    } catch (e, ctx) {
        baml.panics.DivisionByZero => `division|${Frames(ctx)}`,
    };
    let ordinary = CatchPanicDivide(101) catch (e) { _ => -2, };
    baml.io.println(`${result}|${ordinary}`);
}

function CatchWildcardIndex() -> void {
    let result = CatchPanicIndex(5) catch (e) {
        _ => -1,
    } catch (e, ctx) {
        baml.panics.IndexOutOfBounds => `index|${Frames(ctx)}`,
    };
    baml.io.println(`${result}`);
}

function CatchWildcardReassigned() -> void {
    let result = "none";
    {
        let _ = CatchPanicDivide(0);
    } catch (e) {
        _ => {
            result = e;
            e = "replacement";
            result = `${result}+${e}`;
        },
    } catch_all_panics (p) {
        _ => { result = "panic reached outer"; },
    };
    baml.io.println(result);
}

class CatchErrorA { value: int, }
class CatchErrorB { value: int, }
class CatchErrorC { value: int, }
class CatchErrorD { value: int, }

function CatchBindJumpTable() -> void {
    let result = CatchPanicDivide(0) catch (e) {
        CatchErrorA => 10,
        CatchErrorB => 20,
        CatchErrorC => 30,
        CatchErrorD => 40,
        let other => 99,
    } catch (e, ctx) {
        baml.panics.DivisionByZero => `propagated|${Frames(ctx)}`,
    };
    baml.io.println(`${result}`);
}

function CatchBindChain() -> void {
    let result = CatchPanicDivide(0) catch (e) {
        CatchErrorA => 10,
        CatchErrorB => 20,
        CatchErrorC => 30,
        CatchErrorD => 40,
        let other: let alias => 99,
    } catch (e, ctx) {
        baml.panics.DivisionByZero => `propagated|${Frames(ctx)}`,
    };
    baml.io.println(`${result}`);
}

function CatchMixedUnion() -> void {
    let result = CatchPanicIndex(5) catch (e) {
        CatchErrorA | baml.panics.DivisionByZero => 1,
        _ => 99,
    } catch (e, ctx) {
        baml.panics.IndexOutOfBounds => `propagated|${Frames(ctx)}`,
    };
    let named = CatchPanicDivide(0) catch (e) {
        CatchErrorA | baml.panics.DivisionByZero => 1,
        _ => 99,
    };
    baml.io.println(`${result}|${named}`);
}

function CatchThrownPanic() -> int throws never {
    throw baml.panics.AssertionFailed { message: "raised, not thrown" };
}

function CatchWildcardThrownPanic() -> void {
    let result = CatchThrownPanic() catch (inner) {
        _ => 1,
    } catch (outer, ctx) {
        baml.panics.AssertionFailed => `named|${Frames(ctx)}`,
    };
    baml.io.println(`${result}`);
}

function CatchAllThrownPanic() -> void {
    let result = CatchThrownPanic() catch_all (inner) {
        _ => 1,
    } catch (outer, ctx) {
        baml.panics.AssertionFailed => `named|${Frames(ctx)}`,
    };
    baml.io.println(`${result}`);
}

function CatchUnmatchedChain() -> void {
    let result = CatchPanicIndex(5) catch (first) {
        _ => -1,
    } catch_all (second) {
        _ => -2,
    } catch (outer, ctx) {
        baml.panics.IndexOutOfBounds => `outer|${Frames(ctx)}`,
    };
    baml.io.println(`${result}`);
}

function CatchWildcardThenBoundary() -> void {
    let result = CatchPanicIndex(5) catch (first) {
        _ => -1,
    } catch_all_panics (outer, ctx) {
        _ => `boundary|${Frames(ctx)}`,
    };
    baml.io.println(`${result}`);
}

function CatchOrSource(n: int) -> string throws string {
    match (n) {
        0 => throw "ordinary",
        1 => CatchPanicIndex(5).to_string(),
        _ => baml.sys.panic("another panic"),
    }
}

function CatchOrNamedFirst(n: int) -> string {
    CatchOrSource(n) catch (e) {
        baml.panics.IndexOutOfBounds | _ => "accepted",
    } catch (outer) {
        baml.panics.Panic => "propagated",
    }
}

function CatchOrWildcardFirst(n: int) -> string {
    CatchOrSource(n) catch_all (e) {
        _ | baml.panics.IndexOutOfBounds => "accepted",
    } catch (outer) {
        baml.panics.Panic => "propagated",
    }
}

function CatchOrPanics() -> void {
    for (let n in [0, 1, 2]) {
        let boundary = CatchOrSource(n) catch_all_panics (e) {
            baml.panics.IndexOutOfBounds | _ => "boundary",
        };
        baml.io.println(`${CatchOrNamedFirst(n)}:${CatchOrWildcardFirst(n)}:${boundary}`);
    }
}

// Exercise the library's own wildcard catches through real composite clients.
// The client is entirely offline; the spec supplies only ModelTurnInput.
function CatchCompositeSpec() -> string {
    client: "openai/gpt-4o-mini"
    prompt: `Return a short value. ${ctx.output_format()}`
}

function CatchCompositeInput() -> ai.ModelTurnInput {
    let spec = CatchCompositeSpec@spec();
    ai.ModelTurnInput {
        prompt: spec.prompt_template,
        journal: ai.Journal { log: [] },
        toolbox: spec.tools(),
        output_type: spec.output_type(),
    }
}

class CatchCancelClient {
    attempts: int,
    stream_attempts: int,
    cancel: bool,

    implements ai.Client {
        function requested_model(self) -> string { "catch-cancel" }
        function provider(self) -> string { "test" }
        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid/catch-cancel",
                headers: {},
                body: "{}",
            }
        }
        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            self.attempts += 1;
            if (self.cancel) {
                throw baml.panics.Cancelled { message: "scripted cancellation" };
            }
            throw ai.errors.NetworkFailure {
                status_code: null,
                provider: "catch-cancel",
                detail: "retryable failure",
                raw_body: null,
            };
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            let _ = input;
            self.stream_attempts += 1;
            if (self.cancel) {
                return ai.stream.TurnStream.from_event_source(() -> {
                    throw baml.panics.Cancelled { message: "scripted cancellation" };
                });
            }
            ai.stream.TurnStream.from_chunks(["must-not-open"])
        }
    }
}

function CatchNewCancelClient(cancel: bool) -> CatchCancelClient {
    CatchCancelClient { attempts: 0, stream_attempts: 0, cancel: cancel }
}

function CatchNonstreamCancellation() -> void {
    let retry_inner = CatchNewCancelClient(true);
    let retry = ai.clients.Retry.new(
        retry_inner,
        max_attempts = 3,
        backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
    );
    let retry_outcome = retry.invoke(CatchCompositeInput()) catch_all (e) {
        let cancelled: baml.panics.Cancelled => "cancelled",
        _ => "other",
    };
    let rr_inner = CatchNewCancelClient(true);
    let rr = ai.clients.RoundRobin.new([rr_inner]);
    let rr_outcome = rr.invoke(CatchCompositeInput()) catch_all (e) {
        let cancelled: baml.panics.Cancelled => "cancelled",
        _ => "other",
    };
    let fallback_inner = CatchNewCancelClient(true);
    let backup = CatchNewCancelClient(false);
    let fallback = ai.clients.Fallback { members: [fallback_inner, backup] };
    let fallback_outcome = fallback.invoke(CatchCompositeInput()) catch_all (e) {
        let cancelled: baml.panics.Cancelled => "cancelled",
        _ => "other",
    };
    baml.io.println(
        `retry=${retry_outcome}:${retry_inner.attempts}|round_robin=${rr_outcome}:${rr_inner.attempts}|fallback=${fallback_outcome}:${fallback_inner.attempts}:${backup.attempts}`,
    );
}

function CatchStreamingCancellation() -> void {
    let cancelled = CatchNewCancelClient(true);
    let backup = CatchNewCancelClient(false);
    let fallback = ai.clients.Fallback { members: [cancelled, backup] };
    let outcome = fallback.invoke_stream(CatchCompositeInput()).final_turn().terminal_text() catch (e) {
        let cancelled: baml.panics.Cancelled => cancelled.message,
    };
    baml.io.println(`${outcome ?? "null"}|${cancelled.stream_attempts}:${backup.stream_attempts}`);
}
