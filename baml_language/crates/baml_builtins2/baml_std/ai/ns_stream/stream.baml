//! Streaming: the raw text-delta stream of one model turn (TurnStream), the
//! typed partial-value stream returned to BAML and host SDKs (Stream), and the
//! one-turn streaming runner (from_spec). Streaming is a client capability: a
//! client that can stream implements StreamingClient; the compiler-private
//! `Fn@stream` entry binds the authored function's spec and calls from_spec.
//!
//! Phase 1 scope: one model turn, no tool loop. A spec whose toolbox is not
//! empty is rejected with baml.errors.InvalidArgument.
//!
//! ─── Stream termination contract ─────────────────────────────────────────────
//!
//! A TurnStream can end two ways, and they are NOT the same event:
//!
//!   1. TERMINAL EVENT. The decoder yields `TurnDone` because the provider sent
//!      its explicit end-of-turn marker — OpenAI `[DONE]` / `response.completed`,
//!      Anthropic `message_stop`, Gemini a chunk carrying `finishReason`.
//!      The turn is complete and everything the model produced was received.
//!   2. SOCKET CLOSE. `SseStream.next()` returns null: the connection ended.
//!      With no terminal event before it, that is a TRUNCATED turn — a proxy
//!      timing out, a dropped connection, a provider killing the response
//!      mid-generation. The text received so far is a PREFIX of the answer.
//!
//! By default a TurnStream treats (2) as (1): the stream completes and
//! `final_turn()` reports `StopReason.Complete`. That default exists for
//! protocols with no terminal sentinel at all, and it is unsafe for every
//! protocol that HAS one — a mid-stream disconnect is silently reported to the
//! caller as a successful, complete turn carrying truncated text.
//!
//! A decoder whose protocol has a terminal event therefore opts in to strict
//! termination by passing its provider id as `from_sse`'s `require_terminal`:
//!
//!     ai.stream.TurnStream.from_sse(sse, decode, require_terminal = "anthropic")
//!
//! A strict stream that reaches end-of-connection without ever having decoded a
//! `TurnDone` throws `ai.errors.NetworkFailure` ("stream ended before the
//! provider's terminal event") from `next()` / `final_turn()` instead of
//! fabricating a completed turn. `NetworkFailure.retry_safety()` is `Safe`, and
//! that is honest here: the streaming path runs one turn with no tool loop, and
//! a turn that never finished is never committed to the journal, so nothing
//! local advanced and re-issuing the request replays no effects.
//!
//! Opting in is per call site and additive: a `from_sse` call that omits
//! `require_terminal` keeps the historical close-completes-the-turn behavior,
//! and `from_chunks` (scripted, no wire) is never strict.

/// A client that can render the same request as `invoke` but streamed.
/// Implementors must also implement Client (`requires`).
interface StreamingClient requires root.Client {
    /// InvokeError, not just Failure | UnknownError: a streaming invocation
    /// preserves Timeout/Cancelled/CompilationError exactly like Client.invoke.
    function invoke_stream(
        self,
        input: root.ModelTurnInput,
    ) -> TurnStream throws root.errors.InvokeError;
}

/// One decoded raw SSE event, as produced by `baml.http.SseStream.next()`:
/// the batch string is a JSON array of these objects.
class SseEvent {
    event: string?,
    data: string?,
    id: string?,
}

/// One `SseStream.next()` batch -> the raw SSE events it contains. Provider
/// decoders parse each event's `data` payload with their own envelope classes.
function decode_sse_batch(batch: string) -> SseEvent[] {
    baml.json.from_string<SseEvent[]>(batch)
}

// The provider-agnostic events a decoder yields from one SSE batch.

/// One incremental piece of assistant text. The delta contract: the chunk is
/// NOT the accumulated text so far, it is only the newly arrived piece.
class TextDelta {
    text: string,
}

/// Turn metadata as it arrives on the wire. Providers send stop_reason and
/// token counts on different events; each TurnMeta carries what its event had
/// and TurnStream folds non-null fields together (last write wins). The token
/// counts mean what they mean in `root.events.Usage`.
class TurnMeta {
    stop_reason: root.content.StopReason?,
    input_tokens: int?,
    output_tokens: int?,
    cached_input_tokens: int?,
    cache_write_input_tokens: int?,
    uncached_input_tokens: int?,
    reasoning_tokens: int?,
}

/// The provider's explicit end-of-turn marker — the decoded form of `[DONE]`,
/// `response.completed`, `message_stop`, a chunk with `finishReason`, …
///
/// A decoder for a protocol that HAS such a marker must yield TurnDone when it
/// sees it AND its `from_sse` call must pass `require_terminal` (see the
/// termination contract at the top of this file); the stream then reports a
/// connection that closed early as a failure instead of a complete turn. For a
/// protocol with no terminal marker, TurnDone is optional and a socket close
/// still completes the turn.
class TurnDone {
}

/// Non-empty content that is not assistant text: a reasoning or tool-input
/// delta, or a complete tool-call or file part. There is nothing to deliver to
/// the caller, but the model is producing output, which is what
/// `first_token_timeout` and `stream_token_timeout` measure. A decoder yields
/// it for every content-bearing event that produces no TextDelta; events that
/// carry no content (`message_start`, `response.created`, pings, usage-only
/// frames, empty deltas) yield nothing.
class ContentSeen {
}

/// One provider-agnostic event decoded from an SSE batch: a piece of
/// assistant text, content that is not text, the turn metadata that arrived
/// alongside it, or the provider's end-of-turn marker.
type Event = TextDelta | ContentSeen | TurnMeta | TurnDone;

/// The single end-of-stream marker returned by both raw turn streams and typed
/// function streams. A nominal sentinel is required because null may itself be
/// a valid partial value.
class Done {
}

/// Pulls the next normalized event, or `Done` once the turn is over.
///
/// The callback behind `TurnStream.from_event_source`: it lets a composite
/// client relay a member's events without exposing that member's wire
/// format.
type EventSource = () -> Event | Done throws root.errors.InvokeError;

/// The raw text-delta stream of ONE model turn. Pull-based: nothing is read
/// from the wire until `next()` or `final_turn()` is called.
///
/// `next()` yields DELTA chunks (each item is only the newly arrived text,
/// never the accumulated prefix) and returns Done when the turn is over.
/// `final_turn()` drains the rest of the stream and returns the completed
/// turn: the accumulated text as one Text block, the folded stop_reason, and
/// usage where the wire provided token counts.
class TurnStream {
    // callback-backed streams: composite clients use this to relay normalized
    // events from a selected member without exposing provider wire details.
    _event_source: EventSource?,
    // wire-backed streams: the SSE connection plus a per-provider decoder
    // that turns one `SseStream.next()` batch into StreamEvents.
    _sse: baml.http.SseStream?,
    _decode: ((string) -> Event[] throws root.errors.InvokeError)?,
    // scripted streams (tests): literal delta chunks, no network.
    _chunks: string[],
    _cursor: int,
    // decoded events not yet delivered.
    _pending: Event[],
    _pcursor: int,
    // folded state.
    _text: string,
    _stop_reason: root.content.StopReason?,
    _input_tokens: int?,
    _output_tokens: int?,
    _cached_input_tokens: int?,
    _cache_write_input_tokens: int?,
    _uncached_input_tokens: int?,
    _reasoning_tokens: int?,
    _done: bool,
    // strict termination (see the contract at the top of this file): the
    // provider id when this stream requires an explicit TurnDone, null when a
    // socket close is allowed to complete the turn.
    _require_terminal: string?,
    // whether a TurnDone was actually decoded before the stream ended.
    _saw_terminal: bool,
    // Wire telemetry (LLMCall records), surfaced by final_turn().calls.
    // _calls is the full oldest-first list; composite relays share it by
    // reference to prepend failed legs. _wire_call is THIS stream's own
    // exchange record (set by provider invoke_stream implementations), which
    // ttft/duration stamping and SSE capture mutate in place.
    _calls: root.events.LLMCall[],
    _wire_call: root.events.LLMCall?,
    _call_started: baml.time.Instant?,
    // TTFT base: stamped when the SSE response opened (legacy TTFT semantics);
    // falls back to _call_started when a provider did not set it.
    _ttft_from: baml.time.Instant?,
    // The two token deadlines of a wire-backed stream (see
    // `root.LlmTimeoutOptions`); null is not enforced. `_first_token_timeout`
    // applies until the first non-empty content delta is delivered, and
    // `_stream_token_timeout` between content deltas after it: the two never
    // run at the same time.
    _first_token_timeout: baml.time.Duration?,
    _stream_token_timeout: baml.time.Duration?,
    // How many non-empty content deltas have been delivered.
    _content_deltas: int,
    // How long this stream has waited on the provider since the attempt was
    // dispatched (before the first content delta) or since the last content
    // delta (after it). Only time spent waiting for the provider is added, so
    // a caller that is slow between pulls is not charged.
    _token_waited: baml.time.Duration,
    _capture_sse: bool,
    // Where this stream's token usage is recorded: the span or thread that
    // opened it, not whoever pulls it. 0 records nothing (scripted streams,
    // relays, whose member stream records its own usage, and a stream whose
    // usage is already recorded).
    _usage_target: int,
    // The model the client asked for, which prices the usage.
    _usage_model: string?,

    /// A turn stream over a live SSE connection. `decode` receives the raw
    /// batch string from `sse.next()` (a JSON array of SseEvent objects; see
    /// decode_sse_batch) and yields the events it contains.
    ///
    /// `require_terminal` opts into the strict termination contract: pass the
    /// provider id (`"anthropic"`, `"openai"`, …) when the protocol has an
    /// explicit end-of-turn event that the decoder yields as `TurnDone`. The
    /// stream then throws `ai.errors.NetworkFailure` if the connection ends
    /// before that event arrives, rather than reporting the truncated text as a
    /// completed turn. Leave it null (the default) only for a protocol whose
    /// sole end-of-turn signal IS the socket closing.
    ///
    /// `model` is the model the request names; the stream's token usage is
    /// recorded under it, against `usage_target` (the request's span, from
    /// `trace._usage_target_of`), else against the caller's span.
    ///
    /// `timeout_options` supplies the two token deadlines, which this stream enforces:
    /// `next()` throws `baml.errors.Timeout` with `timeout_type`
    /// `"first_token_timeout"` when no non-empty content delta arrives in
    /// time, and `"stream_token_timeout"` when the gap between two of them is
    /// too long. Content is a non-empty `TextDelta` or a `ContentSeen`.
    /// `dispatched` is when the request attempt was sent (before connecting),
    /// which is where the first-token clock starts; it defaults to now. If the
    /// first-token deadline has already passed, the stream is closed and this
    /// throws that `Timeout`. `timeout_options.timeout` and `timeout_options.connect_timeout`
    /// are the transport's and are not read here.
    function from_sse(
        sse: baml.http.SseStream,
        decode: (string) -> Event[] throws root.errors.InvokeError,
        require_terminal: string? = null,
        model: string? = null,
        usage_target: int? = null,
        timeout_options: root.LlmTimeoutOptions? = null,
        dispatched: baml.time.Instant? = null,
    ) -> TurnStream {
        let first_token_timeout = _token_limit(timeout_options?.first_token_timeout);
        // Everything since dispatch was spent waiting on the provider.
        let waited = match (dispatched) {
            let at: baml.time.Instant => at.elapsed(),
            null => baml.time.Duration.from_nanoseconds(0),
        };
        if let limit: baml.time.Duration = first_token_timeout {
            if (waited.to_nanoseconds() >= limit.to_nanoseconds()) {
                sse.close();
                throw _token_timeout_error(limit, "first_token_timeout")
            }
        }
        TurnStream {
            _event_source: null,
            _sse: sse,
            _decode: decode,
            _chunks: [],
            _cursor: 0,
            _pending: [],
            _pcursor: 0,
            _text: "",
            _stop_reason: null,
            _input_tokens: null,
            _output_tokens: null,
            _cached_input_tokens: null,
            _cache_write_input_tokens: null,
            _uncached_input_tokens: null,
            _reasoning_tokens: null,
            _done: false,
            _require_terminal: require_terminal,
            _saw_terminal: false,
            _calls: [],
            _wire_call: null,
            _call_started: null,
            _ttft_from: null,
            _first_token_timeout: first_token_timeout,
            _stream_token_timeout: _token_limit(timeout_options?.stream_token_timeout),
            _content_deltas: 0,
            _token_waited: waited,
            _capture_sse: false,
            _usage_target: usage_target ?? trace._usage_target(),
            _usage_model: model,
        }
    }

    /// A turn stream fed from literal delta chunks — the scripted counterpart
    /// of a live stream, for tests. There is no wire to be cut, so a scripted
    /// stream is never strict: the turn completes as StopReason.Complete after
    /// the last chunk.
    function from_chunks(chunks: string[]) -> TurnStream {
        TurnStream {
            _event_source: null,
            _sse: null,
            _decode: null,
            _chunks: chunks,
            _cursor: 0,
            _pending: [],
            _pcursor: 0,
            _text: "",
            _stop_reason: null,
            _input_tokens: null,
            _output_tokens: null,
            _cached_input_tokens: null,
            _cache_write_input_tokens: null,
            _uncached_input_tokens: null,
            _reasoning_tokens: null,
            _done: false,
            _require_terminal: null,
            _saw_terminal: false,
            _calls: [],
            _wire_call: null,
            _call_started: null,
            _ttft_from: null,
            _first_token_timeout: null,
            _stream_token_timeout: null,
            _content_deltas: 0,
            _token_waited: baml.time.Duration.from_nanoseconds(0),
            _capture_sse: false,
            _usage_target: 0,
            _usage_model: null,
        }
    }

    /// A stream backed by one normalized-event callback. Reliability clients
    /// use this to relay a chosen member's deltas and final metadata while
    /// retaining TurnStream's ordinary accumulation and termination behavior.
    /// The source itself owns any retry/fallback policy; once it returns a
    /// TextDelta, that delta is immediately visible to the caller.
    function from_event_source(next_event: EventSource) -> TurnStream {
        TurnStream {
            _event_source: next_event,
            _sse: null,
            _decode: null,
            _chunks: [],
            _cursor: 0,
            _pending: [],
            _pcursor: 0,
            _text: "",
            _stop_reason: null,
            _input_tokens: null,
            _output_tokens: null,
            _cached_input_tokens: null,
            _cache_write_input_tokens: null,
            _uncached_input_tokens: null,
            _reasoning_tokens: null,
            _done: false,
            _require_terminal: null,
            _saw_terminal: false,
            _calls: [],
            _wire_call: null,
            _call_started: null,
            _ttft_from: null,
            _first_token_timeout: null,
            _stream_token_timeout: null,
            _content_deltas: 0,
            _token_waited: baml.time.Duration.from_nanoseconds(0),
            _capture_sse: false,
            _usage_target: 0,
            _usage_model: null,
        }
    }

    /// (internal) Stamp time-to-first-token on this stream's wire record, once.
    function _stamp_first_delta(self) -> void {
        if let call: root.events.LLMCall = self._wire_call {
            if (call.timing.ttft_ms == null) {
                if let base: baml.time.Instant = self._ttft_from ?? self._call_started {
                    call.timing.ttft_ms = base.elapsed().to_milliseconds();
                }
            }
        }
    }

    /// (internal) A non-empty content delta was delivered: the first-token
    /// deadline is met, and the stream-token clock starts over.
    function _saw_content(self) -> void {
        self._content_deltas += 1;
        self._token_waited = baml.time.Duration.from_nanoseconds(0);
    }

    /// (internal) One wire pull, bounded by whichever token deadline is
    /// running: `first_token_timeout` until the first content delta, then
    /// `stream_token_timeout`. The clock only runs here, while waiting on the
    /// provider. A batch with no content in it does not reset it, so the next
    /// pull gets what is left of the same budget.
    ///
    /// The transport's own `Timeout` (the request's end-to-end limit) is
    /// carried out of the timed body as a value so it is never mistaken for
    /// the token deadline, then rethrown unchanged.
    function _pull(self, sse: baml.http.SseStream) -> string? {
        let timeout_type = if (self._content_deltas > 0) {
            "stream_token_timeout"
        } else {
            "first_token_timeout"
        };
        let active = if (self._content_deltas > 0) {
            self._stream_token_timeout
        } else {
            self._first_token_timeout
        };
        match (active) {
            null => sse.next(),
            let limit: baml.time.Duration => {
                let remaining = limit.sub(self._token_waited);
                if (remaining.to_nanoseconds() <= 0n) {
                    self._finish();
                    throw _token_timeout_error(limit, timeout_type)
                }
                let pull_started = baml.time.Instant.now();
                let pulled = baml.future.with_timeout(remaining, () -> {
                    sse.next() catch (e) {
                        let transport: baml.errors.Timeout => transport,
                    }
                }) catch (e) {
                    baml.errors.Timeout => {
                        self._finish();
                        throw _token_timeout_error(limit, timeout_type)
                    },
                };
                self._token_waited = self._token_waited.add(pull_started.elapsed());
                match (pulled) {
                    let transport: baml.errors.Timeout => throw transport,
                    let batch: string => batch,
                    null => null,
                }
            },
        }
    }

    /// (internal) Append a raw SSE batch to the wire record when capture is on. Telemetry
    /// must never fail the stream, so a malformed batch is skipped.
    function _capture_batch(self, batch: string) -> void {
        if (!self._capture_sse) {
            return;
        }
        if let call: root.events.LLMCall = self._wire_call {
            let none: SseEvent[] = [];
            let decoded = decode_sse_batch(batch) catch_all (e) {
                _ => none,
            };
            let records = call.sse_responses ?? [];
            for (let e in decoded) {
                records.push(root.events.SseEventRecord { event: e.event, data: e.data, id: e.id });
            }
            call.sse_responses = records;
        }
    }

    /// (internal) Marks the turn over and releases the SSE connection
    /// behind it, if this stream has one.
    function _finish(self) -> void {
        self._done = true;
        if let sse: baml.http.SseStream = self._sse {
            sse.close();
        }
        self._record_tokens();
    }

    /// (internal) Record this turn's tokens against `_usage_target`, once, as
    /// soon as the stream ends: whether or not anyone asks for the final turn.
    function _record_tokens(self) -> void {
        let target = self._usage_target;
        if (target != 0 && (self._input_tokens != null || self._output_tokens != null)) {
            self._usage_target = 0;
            // Like a non-streamed turn: full-rate input, then the cache split.
            let _ = trace._record_usage(
                target,
                self._usage_model,
                self._uncached_input_tokens ?? self._input_tokens ?? 0,
                self._output_tokens ?? 0,
                self._cached_input_tokens,
                self._cache_write_input_tokens,
                self._reasoning_tokens,
            );
        }
    }

    /// (internal) The strict-termination check: a no-op unless this stream required an
    /// explicit terminal event and the stream ended without one, in which case
    /// the truncated turn is reported as the transport failure it is instead of
    /// being passed off as complete. Called on every end-of-stream path.
    function _check_terminal(self) -> void {
        if let provider: string = self._require_terminal {
            if (!self._saw_terminal) {
                let partial: string? = if (self._text == "") {
                    null
                } else {
                    self._text
                };
                throw root.errors.NetworkFailure { status_code: null,
                    provider: provider,
                    detail: `stream ended before the provider's terminal event: the connection closed after ${self._text.length()} characters of assistant text with no end-of-turn event, so the turn is truncated`,
                    raw_body: partial,
                }
            }
        }
    }

    /// All READY text deltas, or Done when the turn is over. Metadata events
    /// are folded silently; calling next() after Done keeps returning Done.
    ///
    /// Drain-ready-then-deliver: every event already decoded (the wire layer
    /// returns whole batches — everything that queued while the caller was
    /// busy) is returned as ONE batch, and the wire is only pulled when no
    /// deliverable text is in hand. Delta boundaries are preserved — every
    /// element is one provider text delta, so consumers that want each SSE
    /// delta individually iterate the batch. A consumer that keeps up sees
    /// one-element batches; a consumer that falls behind (e.g. a partial
    /// parse slower than the token interval) gets the whole backlog in one
    /// return instead of replaying it delta by delta — the pull-model
    /// equivalent of the legacy engine's latest-snapshot watch channel.
    function next(self) -> string[] | Done {
        let ready: string[] = [];
        while (true) {
            if (self._done) {
                return Done {};
            }
            // deliver every decoded event before pulling more from the wire
            while (self._pcursor < self._pending.length()) {
                let e = self._pending.at(self._pcursor);
                self._pcursor += 1;
                match (e) {
                    let d: TextDelta => {
                        self._stamp_first_delta();
                        if (d.text != "") {
                            self._saw_content();
                        }
                        self._text = `${self._text}${d.text}`;
                        ready.push(d.text);
                        null
                    },
                    let seen: ContentSeen => {
                        self._saw_content();
                        null
                    },
                    let m: TurnMeta => {
                        if let s: root.content.StopReason = m.stop_reason {
                            self._stop_reason = s;
                        }
                        if let i: int = m.input_tokens {
                            self._input_tokens = i;
                        }
                        if let o: int = m.output_tokens {
                            self._output_tokens = o;
                        }
                        if let c: int = m.cached_input_tokens {
                            self._cached_input_tokens = c;
                        }
                        if let w: int = m.cache_write_input_tokens {
                            self._cache_write_input_tokens = w;
                        }
                        if let u: int = m.uncached_input_tokens {
                            self._uncached_input_tokens = u;
                        }
                        if let r: int = m.reasoning_tokens {
                            self._reasoning_tokens = r;
                        }
                        null
                    },
                    let d: TurnDone => {
                        self._saw_terminal = true;
                        self._finish();
                        // text decoded ahead of the terminal is delivered
                        // now; Done follows on the next call (_done is set).
                        if (ready.length() > 0) {
                            return ready;
                        }
                        return Done {};
                    },
                    null => null,
                };
            }
            // never block on the wire while holding deliverable text
            if (ready.length() > 0) {
                return ready;
            }
            if let source: EventSource = self._event_source {
                match (source()) {
                    let event: Event => {
                        self._pending.push(event);
                        null
                    },
                    let done: Done => {
                        self._finish();
                        return done;
                    },
                };
                continue;
            }
            match (self._sse) {
                let sse: baml.http.SseStream => {
                    match (self._pull(sse)) {
                        null => {
                            // end of connection. On a strict stream with no
                            // TurnDone decoded this is a truncated turn, and
                            // _check_terminal throws rather than completing it.
                            self._finish();
                            self._check_terminal();
                            return Done {};
                        },
                        let batch: string => {
                            self._capture_batch(batch);
                            if let decode: (string) -> Event[] throws root.errors.InvokeError = self._decode {
                                for (let e in decode(batch)) {
                                    self._pending.push(e);
                                }
                                null
                            } else {
                                null
                            };
                            null
                        },
                    };
                    null
                },
                null => {
                    // scripted: each chunk is one delta
                    match (self._chunks.at(self._cursor)) {
                        let c: string => {
                            self._stamp_first_delta();
                            self._cursor += 1;
                            self._text = `${self._text}${c}`;
                            return [c];
                        },
                        null => {
                            self._finish();
                            return Done {};
                        },
                    }
                },
            };
        }
        baml.sys.panic("unreachable")
    }

    /// Drain the stream and return the completed turn. Safe to call at any
    /// point (including after Done); calling twice returns an equal turn.
    ///
    /// On a strict stream (from_sse's `require_terminal`) that ended without the
    /// provider's terminal event this throws ai.errors.NetworkFailure instead of
    /// returning a turn — the stop_reason default below must never turn a
    /// truncated stream into a `StopReason.Complete` answer.
    function final_turn(self) -> root.ModelTurn {
        while (true) {
            match (self.next()) {
                let d: Done => {
                    break;
                },
                _ => null,
            };
        }
        // next() throws on the truncated path it discovers itself; this also
        // covers a stream finished by some other route (a Done delivered from
        // an already-closed stream) before the terminal event was seen.
        self._check_terminal();
        let usage: root.events.Usage? = if (self._input_tokens != null || self._output_tokens != null) {
            root.events.Usage {
                input_tokens: self._input_tokens ?? 0,
                output_tokens: self._output_tokens ?? 0,
                cached_input_tokens: self._cached_input_tokens,
                reasoning_tokens: self._reasoning_tokens,
                cache_write_input_tokens: self._cache_write_input_tokens,
                uncached_input_tokens: self._uncached_input_tokens,
            }
        } else {
            null
        };
        self._record_tokens();
        if let call: root.events.LLMCall = self._wire_call {
            if (call.timing.duration_ms == null) {
                if let started: baml.time.Instant = self._call_started {
                    call.timing.duration_ms = started.elapsed().to_milliseconds();
                }
            }
        }
        root.ModelTurn {
            content: [root.content.Text { text: self._text }],
            stop_reason: self._stop_reason ?? root.content.StopReason.Complete,
            usage: usage,
            calls: self._calls,
        }
    }
}

/// (internal) A configured token deadline, or null when there is none: a zero
/// or negative duration means no limit.
function _token_limit(limit: baml.time.Duration?) -> baml.time.Duration? {
    match (limit) {
        null => null,
        let configured: baml.time.Duration => {
            if (configured.to_nanoseconds() > 0n) {
                configured
            } else {
                null
            }
        },
    }
}

/// (internal) The error for an elapsed token deadline. `timeout_type` is the
/// `LlmTimeoutOptions` field that set it.
function _token_timeout_error(
    limit: baml.time.Duration,
    timeout_type: string,
) -> baml.errors.Timeout {
    let waiting_for = if (timeout_type == "first_token_timeout") {
        "the first content from the model"
    } else {
        "more content from the model"
    };
    baml.errors.Timeout {
        message: `no ${waiting_for} within ${limit.to_milliseconds()}ms (${timeout_type})`,
        duration: limit,
        timeout_type: timeout_type,
    }
}

/// The typed partial-value stream returned by an LLM function's `@stream`
/// projection. This is the one canonical host-stream class; tagged handles carry
/// its FQN across FFI so host bridges do not need to hardcode a namespace.
///
/// Partial and final values share one type: a partial is the declared type parsed
/// from the text received so far.
class Stream<T> {
    _next_text: () -> string? throws root.errors.InvokeError,
    _cache: baml.sap._ParseCache<T>,
    _text: string,
    _finished: bool,

    /// Return the next parsed partial value, or Done after EOF.
    function next(self) -> T | Done {
        while (true) {
            if (self._finished) {
                return Done {};
            }
            match (self._next_text()) {
                null => {
                    self._finished = true;
                    return Done {};
                },
                let s: string => {
                    self._text = self._text + s;
                    let parsed: T | baml.sap._NoYield = self._cache._parse_partial(self._text);
                    match (parsed) {
                        baml.sap._NoYield => {},
                        let p: T => {
                            return p;
                        },
                    }
                },
            }
        }
        baml.sys.panic("unreachable")
    }

    /// Drain the stream and parse the accumulated text as the final value.
    function final(self) -> T {
        while (!self._finished) {
            match (self._next_text()) {
                null => {
                    self._finished = true
                },
                let s: string => {
                    self._text = self._text + s
                },
            }
        }
        self._cache._parse_final(self._text)
    }
}

/// One-turn streaming over a spec: render the spec's prompt exactly as the
/// default runner's first turn would, require a streaming-capable client, and
/// return the typed partial stream. The compiler-private `Fn@stream` entry
/// calls this function; host stream helpers reach that companion through the
/// normal bridge.
///
/// Streaming does not run the tool loop yet: a spec with a non-empty toolbox
/// is rejected. The client override is typed as the capability, so passing a
/// non-streaming client fails at the call site; the spec's DEFAULT client is
/// only checked at runtime (ai.errors.StreamingUnsupported when it cannot
/// stream).
function from_spec<T>(
    spec: root.FunctionSpec<T>,
    client: StreamingClient? = null,
    on_event: ((root.events.Event) -> void)? = null,
) -> Stream<T> {
    if (!spec.tools().is_empty()) {
        throw baml.errors.InvalidArgument {
            message: `streaming does not run the tool loop yet: ${spec.name()} has a non-empty toolbox`,
        }
    }
    let selected: StreamingClient = match (client) {
        let s: StreamingClient => s,
        null => {
            match (spec.default_client) {
                let s: StreamingClient => s,
                _ => {
                    throw root.errors.StreamingUnsupported {
                        client_id: spec.default_client.id(),
                    }
                },
            }
        },
    };
    // StreamingClient requires Client, so the downcast always succeeds.
    let provider = match (selected) {
        let c: root.Client => c.id(),
    };
    let emit = root.events.guard(on_event);
    let settled = false;
    if let cb: (root.events.Event) -> void throws never = emit {
        cb(root.events.RunStarted { spec_name: spec.name(), arguments: spec.arguments() });
    }
    let turns = selected.invoke_stream(
        root.ModelTurnInput {
            prompt: spec.prompt_template,
            journal: root.Journal.new(spec),
            toolbox: spec.tools(),
            output_type: spec.output_type(),
        },
    );
    // The host-facing stream is `ai.stream.Stream`. It is provider-agnostic:
    // all it needs is a way to pull the next text delta from TurnStream.
    Stream {
        _next_text: () -> {
            match (turns.next()) {
                let d: Done => {
                    // Settle: fold the completed turn (this is where the
                    // previously-discarded streamed usage is recovered) and
                    // emit the same events the Agent path emits. The flag
                    // guards re-entry: a listener that pulls this same stream
                    // (e.g. calls final() mid-callback) must not re-settle.
                    if (!settled) {
                        settled = true;
                        if let cb: (root.events.Event) -> void throws never = emit {
                            let turn = turns.final_turn();
                            cb(
                                root.events.AssistantMessage {
                                    content: turn.content,
                                    client_id: provider,
                                },
                            );
                            if let u: root.events.Usage = turn.usage {
                                cb(u);
                            }
                            for (let call in turn.calls) {
                                cb(call);
                            }
                        }
                    }
                    null
                },
                let batch: string[] => batch.join(""),
            } catch_all (e) {
                _ => throw root.errors.normalize(e),
            }
        },
        _cache: baml.sap._new_parse_cache<T>(),
        _text: "",
        _finished: false,
    }
}
