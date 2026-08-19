// Reliability composes at the client boundary: wrapper clients.
// Mirrors _planv2/pages/02_guides/03_clients/04_reliability.md.

// ---------------------------------------------------------------------------
// Dynamic client selection.
//
// A `client:` position accepts more than a constructed client: the same
// "provider/model" shorthand the compiler understands, and a late-bound
// `baml.env.Ref` so a deployment can pick its model with an environment
// variable that is read at CALL time rather than snapshotted during `$init`.

/// What a `client:` position accepts: an already-built client, a
/// `"provider/model"` shorthand string, or a `baml.env.Ref` naming a variable
/// that holds one. `ai.clients.resolve` turns any of them into a `root.Client`.
type ClientSelector = root.Client | string | baml.env.Ref;

/// Resolve a `ClientSelector` to the client to call, at CALL time.
///
///   * a `root.Client` is returned unchanged (identity);
///   * a string is the `"provider/model"` shorthand, constructed here — the
///     runtime twin of the compiler's literal-`client:` lowering. An unknown
///     prefix throws `root.errors.InvalidRequest` listing the valid ones;
///   * a `baml.env.Ref` is read now and its value takes the string path. An
///     UNSET variable is a typed `root.errors.InvalidRequest` naming it — not
///     a panic, so a `catch_all` at the call boundary can still fall back.
///
/// This is what makes `client: env.MODEL.get() ?? "openai/gpt-4o-mini"` work:
/// nothing about the selection is decided until the call happens.
function resolve(selector: ClientSelector) -> root.Client {
    match (selector) {
        let shorthand: string => root.internal._from_shorthand(shorthand),
        let named: baml.env.Ref => {
            if let value: string = named.get() {
                root.internal._from_shorthand(value)
            } else {
                throw root.errors.InvalidRequest {
                    provider: "ai.clients",
                    status_code: null,
                    detail: `client selector env.${named.name} is not set: point it at a "provider/model" string (valid prefixes are ${root.internal._shorthand_prefixes()}), or give the client: position a value with a fallback — env.${named.name}.get() ?? "openai/gpt-4o-mini"`,
                    raw_body: null,
                }
            }
        },
        let existing: root.Client => existing,
    }
}

// The default judgment: a request the provider rejected does not become
// valid by resending.
function default_retry_judgment(f: root.errors.Failure) -> bool {
    match (f) {
        let r: root.errors.Refused => false,
        let i: root.errors.InvalidRequest => false,
        let p: root.errors.ParseFailed => false,
        let f: root.errors.FinishReasonError => false,
        _ => true,
    }
}

class Backoff {
    initial_ms: int,
    multiplier: int,
    max_ms: int,

    function new(initial_ms: int = 250, multiplier: int = 2, max_ms: int = 10000) -> Backoff {
        Backoff { initial_ms: initial_ms, multiplier: multiplier, max_ms: max_ms }
    }

    // The delay before attempt N (first retry is attempt 1). A
    // root.errors.RateLimited.retry_after_ms hint overrides the computed value.
    function delay_ms(self, attempt: int, f: root.errors.Failure) -> int {
        let hinted = match (f) {
            let r: root.errors.RateLimited => r.retry_after_ms,
            _ => null,
        };
        if let h: int = hinted {
            h
        } else {
            let d = self.initial_ms;
            let i = 1;
            while (i < attempt) {
                d = d * self.multiplier;
                i = i + 1;
            }
            if (d > self.max_ms) {
                self.max_ms
            } else {
                d
            }
        }
    }
}

// Streaming retries use the same safety judgment and backoff as invoke, but a
// new stream may be opened only before the first text delta is returned.
class Retry {
    inner: root.Client,
    max_attempts: int,
    retry_if: (root.errors.Failure) -> bool throws never,
    backoff: Backoff,

    function new(
        inner: root.Client,
        max_attempts: int = 3,
        retry_if: ((root.errors.Failure) -> bool throws never)? = null,
        backoff: Backoff? = null,
    ) -> Retry {
        Retry {
            inner: inner,
            max_attempts: max_attempts,
            retry_if: retry_if ?? default_retry_judgment,
            backoff: backoff ?? Backoff.new(),
        }
    }

    function _attempt(self, input: root.ModelTurnInput, remaining: int) -> root.ModelTurn {
        self.inner.invoke(input) catch_all (e) {
            let f: root.errors.Failure => {
                // the safety gate is not overridable; the judgment is
                if (
                    remaining > 1
                        && f.retry_safety() == root.errors.RetrySafety.Safe
                        && self.retry_if(f)
                ) {
                    let attempt = self.max_attempts - remaining + 1;
                    let _ = baml.sys.sleep(
                        baml.time.Duration.from_milliseconds(self.backoff.delay_ms(attempt, f)),
                    ) catch_all (se) {
                        _ => null,
                    };
                    self._attempt(input, remaining - 1)
                } else {
                    throw e
                }
            },
            _ => throw e, // untyped failures are never retried
        }
    }

    implements root.Client {
        function id(self) -> string {
            self.inner.id()
        }

        // Rendering is a pure preview of the inner client. It deliberately
        // does not apply retry policy or backoff because no request is sent.
        function render(self, input: root.ModelTurnInput) -> baml.http.Request {
            self.inner.render(input) catch_all (e) {
                _ => throw root.errors.normalize(e),
            }
        }

        function invoke(self, input: root.ModelTurnInput) -> root.ModelTurn {
            self._attempt(input, self.max_attempts) catch_all (e) {
                _ => throw root.errors.normalize_invoke(e),
            }
        }
    }

    implements root.stream.StreamingClient {
        function invoke_stream(self, input: root.ModelTurnInput) -> root.stream.TurnStream {
            let remaining = self.max_attempts;
            let open = () -> root.stream.TurnStream {
                match (self.inner) {
                    let streaming: root.stream.StreamingClient => streaming.invoke_stream(input),
                    let member: root.Client => {
                        throw root.errors.StreamingUnsupported { client_id: member.id() }
                    },
                }
            };
            let retry = (failure: root.errors.Failure) -> bool {
                if (failure is root.errors.StreamingUnsupported) {
                    false
                } else if (
                    remaining > 1
                        && failure.retry_safety() == root.errors.RetrySafety.Safe
                        && self.retry_if(failure)
                ) {
                    let attempt = self.max_attempts - remaining + 1;
                    let _ = baml.sys.sleep(
                        baml.time.Duration.from_milliseconds(
                            self.backoff.delay_ms(attempt, failure),
                        ),
                    ) catch_all (e) {
                        let cancelled: baml.panics.Cancelled => throw cancelled,
                        _ => null,
                    };
                    remaining -= 1;
                    true
                } else {
                    false
                }
            };
            let relay = StreamRelay {
                open: open,
                retry: retry,
                current: null,
                emitted: false,
                terminal: [],
                terminal_cursor: 0,
                finished: false,
            };
            root.stream.TurnStream.from_event_source(relay.next_event)
        }
    }
}

// Rotates invocations across members. Replaces the legacy round-robin
// provider: distribution is a client-boundary concern, exactly like Retry
// and Fallback. The counter mutates on the shared value, so every call
// site holding this client participates in one rotation. A streaming call
// selects and advances the counter exactly once; a failure from that selected
// stream never rotates again within the same call.
class RoundRobin {
    members: root.Client[],
    counter: int,

    function new(members: root.Client[]) -> RoundRobin {
        if (members.length() == 0) {
            throw root.errors.InvalidRequest {
                provider: "roundrobin",
                status_code: null,
                detail: "RoundRobin needs at least one member",
                raw_body: null,
            }
        }
        RoundRobin { members: members, counter: 0 }
    }

    implements root.Client {
        function id(self) -> string {
            if let c: root.Client = self.members.at(0) {
                c.id()
            } else {
                "round-robin"
            }
        }

        // Rendering is deterministic and side-effect free: preview the first
        // member rather than mutating the invocation rotation counter.
        function render(self, input: root.ModelTurnInput) -> baml.http.Request {
            match (self.members.at(0)) {
                let c: root.Client => c.render(input),
                null => {
                    throw root.errors.InvalidRequest {
                        provider: "roundrobin",
                        status_code: null,
                        detail: "RoundRobin needs at least one member",
                        raw_body: null,
                    }
                },
            } catch_all (e) {
                _ => throw root.errors.normalize(e),
            }
        }

        function invoke(self, input: root.ModelTurnInput) -> root.ModelTurn {
            {
                // Guard before the modulo: an empty rotation would otherwise
                // panic with DivisionByZero, which bypasses every catch.
                if (self.members.length() == 0) {
                    throw root.errors.InvalidRequest {
                        provider: "roundrobin",
                        status_code: null,
                        detail: "RoundRobin needs at least one member",
                        raw_body: null,
                    }
                }
                let index = self.counter % self.members.length();
                self.counter = self.counter + 1;
                match (self.members.at(index)) {
                    let c: root.Client => c.invoke(input),
                    null => {
                        throw root.errors.InvalidRequest {
                            provider: "roundrobin",
                            status_code: null,
                            detail: "RoundRobin needs at least one member",
                            raw_body: null,
                        }
                    },
                }
            } catch_all (e) {
                _ => throw root.errors.normalize_invoke(e),
            }
        }
    }

    implements root.stream.StreamingClient {
        function invoke_stream(self, input: root.ModelTurnInput) -> root.stream.TurnStream {
            {
                if (self.members.length() == 0) {
                    throw root.errors.InvalidRequest {
                        provider: "roundrobin",
                        status_code: null,
                        detail: "RoundRobin needs at least one member",
                        raw_body: null,
                    }
                }
                let index = self.counter % self.members.length();
                self.counter += 1;
                match (self.members.at(index)) {
                    let streaming: root.stream.StreamingClient => streaming.invoke_stream(input),
                    let member: root.Client => {
                        throw root.errors.StreamingUnsupported { client_id: member.id() }
                    },
                    null => {
                        throw root.errors.InvalidRequest {
                            provider: "roundrobin",
                            status_code: null,
                            detail: "RoundRobin needs at least one member",
                            raw_body: null,
                        }
                    },
                }
            } catch_all (e) {
                _ => throw root.errors.normalize_invoke(e),
            }
        }
    }
}

// Streaming fallback advances in member order only for a typed, safely
// retryable failure that occurs before the first text delta is returned.
class Fallback {
    members: root.Client[],

    function _try(self, input: root.ModelTurnInput, index: int) -> root.ModelTurn {
        match (self.members.at(index)) {
            let c: root.Client => {
                c.invoke(input) catch_all (e) {
                    let f: root.errors.Failure => {
                        if (
                            index + 1 < self.members.length()
                                && f.retry_safety() == root.errors.RetrySafety.Safe
                        ) {
                            self._try(input, index + 1)
                        } else {
                            throw e
                        }
                    },
                    _ => throw e,
                }
            },
            null => {
                throw root.errors.InvalidRequest {
                    provider: "fallback",
                    status_code: null,
                    detail: "Fallback needs at least one member",
                    raw_body: null,
                }
            },
        }
    }

    implements root.Client {
        function id(self) -> string {
            // NOTE: `?.method()` on an interface-typed optional trips a VM bug;
            // if-let is the reliable form.
            if let c: root.Client = self.members.at(0) {
                c.id()
            } else {
                "fallback"
            }
        }

        // Rendering previews the first fallback member. It never probes later
        // members because request construction itself must remain pure.
        function render(self, input: root.ModelTurnInput) -> baml.http.Request {
            match (self.members.at(0)) {
                let c: root.Client => c.render(input),
                null => {
                    throw root.errors.InvalidRequest {
                        provider: "fallback",
                        status_code: null,
                        detail: "Fallback needs at least one member",
                        raw_body: null,
                    }
                },
            } catch_all (e) {
                _ => throw root.errors.normalize(e),
            }
        }

        function invoke(self, input: root.ModelTurnInput) -> root.ModelTurn {
            self._try(input, 0) catch_all (e) {
                _ => throw root.errors.normalize_invoke(e),
            }
        }
    }

    implements root.stream.StreamingClient {
        function invoke_stream(self, input: root.ModelTurnInput) -> root.stream.TurnStream {
            let index = 0;
            let open = () -> root.stream.TurnStream {
                match (self.members.at(index)) {
                    let streaming: root.stream.StreamingClient => streaming.invoke_stream(input),
                    let member: root.Client => {
                        throw root.errors.StreamingUnsupported { client_id: member.id() }
                    },
                    null => {
                        throw root.errors.InvalidRequest {
                            provider: "fallback",
                            status_code: null,
                            detail: "Fallback needs at least one member",
                            raw_body: null,
                        }
                    },
                }
            };
            let advance = (failure: root.errors.Failure) -> bool {
                if (
                    index + 1 < self.members.length()
                        && failure.retry_safety() == root.errors.RetrySafety.Safe
                ) {
                    index += 1;
                    true
                } else {
                    false
                }
            };
            let relay = StreamRelay {
                open: open,
                retry: advance,
                current: null,
                emitted: false,
                terminal: [],
                terminal_cursor: 0,
                finished: false,
            };
            root.stream.TurnStream.from_event_source(relay.next_event)
        }
    }
}

// Pulls one selected stream through a reliability policy. A typed, safely
// retryable failure may replace the selected stream only before the first text
// delta has been returned. Once any delta is visible, every later failure is
// terminal: replaying would duplicate a prefix and there is no resumable-stream
// contract at the client boundary.
class StreamRelay {
    open: () -> root.stream.TurnStream throws root.errors.InvokeError,
    retry: (root.errors.Failure) -> bool throws root.errors.InvokeError,
    current: root.stream.TurnStream?,
    emitted: bool,
    terminal: root.stream.StreamEvent[],
    terminal_cursor: int,
    finished: bool,

    function close_current(self) -> void {
        if let stream: root.stream.TurnStream = self.current {
            stream._finish();
        }
    }

    function recover(self, error: root.errors.RaisedInClient) -> void {
        match (root.errors.normalize_invoke(error)) {
            let timeout: baml.errors.Timeout => {
                self.close_current();
                throw timeout
            },
            let cancelled: baml.panics.Cancelled => {
                self.close_current();
                throw cancelled
            },
            let failure: root.errors.Failure => {
                if (!self.emitted && self.retry(failure)) {
                    self.close_current();
                    self.current = null;
                } else {
                    self.close_current();
                    throw failure
                }
            },
            let unknown_error: baml.errors.UnknownError => {
                self.close_current();
                throw unknown_error
            },
            let compilation: baml.reflect.errors.CompilationError => {
                self.close_current();
                throw compilation
            },
        }
    }

    function next_event(self) -> root.stream.StreamEvent | root.stream.Done {
        if (self.terminal_cursor < self.terminal.length()) {
            let event = self.terminal.at(self.terminal_cursor)
                ?? baml.sys.panic("StreamRelay lost a terminal event");
            self.terminal_cursor += 1;
            return event;
        }
        if (self.finished) {
            return root.stream.Done {  };
        }
        if (self.current == null) {
            let opened: root.stream.TurnStream | StreamRelayError = self.open() catch_all (e) {
                _ => StreamRelayError { error: e },
            };
            if let failed: StreamRelayError = opened {
                self.recover(failed.error);
                return self.next_event();
            } else if let stream: root.stream.TurnStream = opened {
                self.current = stream;
            }
        }
        if let stream: root.stream.TurnStream = self.current {
            let next: string | root.stream.Done | StreamRelayError = stream.next() catch_all (e) {
                _ => StreamRelayError { error: e },
            };
            match (next) {
                let text: string => {
                    self.emitted = true;
                    root.stream.TextDelta { text: text }
                },
                let done: root.stream.Done => {
                    let final: root.ModelTurn | StreamRelayError = stream.final_turn() catch_all (e) {
                        _ => StreamRelayError { error: e },
                    };
                    if let failed: StreamRelayError = final {
                        self.recover(failed.error);
                        return self.next_event();
                    } else if let turn: root.ModelTurn = final {
                        self.finished = true;
                        self.terminal
                            = [
                                root.stream.TurnMeta {
                                    stop_reason: turn.stop_reason,
                                    input_tokens: turn.usage?.input_tokens,
                                    output_tokens: turn.usage?.output_tokens,
                                },
                                root.stream.TurnDone {  },
                            ];
                        return self.next_event();
                    }
                    done
                },
                let failed: StreamRelayError => {
                    self.recover(failed.error);
                    self.next_event()
                },
            }
        } else {
            baml.sys.panic("StreamRelay has no current stream")
        }
    }
}

class StreamRelayError {
    error: root.errors.RaisedInClient,
}
