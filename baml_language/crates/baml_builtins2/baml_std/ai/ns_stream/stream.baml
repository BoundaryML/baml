// Streaming: the raw text-delta stream of one model turn (TurnStream), the
// typed partial-value stream returned to BAML and host SDKs (Stream), and the
// one-turn streaming runner (from_spec). Streaming is a client capability: a
// client that can stream implements StreamingClient; `Fn$stream` companions
// call from_spec.
//
// Phase 1 scope: one model turn, no tool loop. A spec whose toolbox is not
// empty is rejected with baml.errors.InvalidArgument.
//
// ─── Stream termination contract ─────────────────────────────────────────────
//
// A TurnStream can end two ways, and they are NOT the same event:
//
//   1. TERMINAL EVENT. The decoder yields `TurnDone` because the provider sent
//      its explicit end-of-turn marker — OpenAI `[DONE]` / `response.completed`,
//      Anthropic `message_stop`, Gemini a chunk carrying `finishReason`.
//      The turn is complete and everything the model produced was received.
//   2. SOCKET CLOSE. `SseStream.next()` returns null: the connection ended.
//      With no terminal event before it, that is a TRUNCATED turn — a proxy
//      timing out, a dropped connection, a provider killing the response
//      mid-generation. The text received so far is a PREFIX of the answer.
//
// By default a TurnStream treats (2) as (1): the stream completes and
// `final_turn()` reports `StopReason.Complete`. That default exists for
// protocols with no terminal sentinel at all, and it is unsafe for every
// protocol that HAS one — a mid-stream disconnect is silently reported to the
// caller as a successful, complete turn carrying truncated text.
//
// A decoder whose protocol has a terminal event therefore opts in to strict
// termination by passing its provider id as `from_sse`'s `require_terminal`:
//
//     ai.stream.TurnStream.from_sse(sse, decode, require_terminal = "anthropic")
//
// A strict stream that reaches end-of-connection without ever having decoded a
// `TurnDone` throws `ai.errors.NetworkFailure` ("stream ended before the
// provider's terminal event") from `next()` / `final_turn()` instead of
// fabricating a completed turn. `NetworkFailure.retry_safety()` is `Safe`, and
// that is honest here: the streaming path runs one turn with no tool loop, and
// a turn that never finished is never committed to the journal, so nothing
// local advanced and re-issuing the request replays no effects.
//
// Opting in is per call site and additive: a `from_sse` call that omits
// `require_terminal` keeps the historical close-completes-the-turn behavior,
// and `from_chunks` (scripted, no wire) is never strict.

// A client that can render the same request as `invoke` but streamed.
// Implementors must also implement Client (`requires`).
interface StreamingClient requires root.Client {
    function invoke_stream(self, input: root.ModelTurnInput) -> TurnStream throws root.errors.InvokeError
}

// One decoded raw SSE event, as produced by `baml.http.SseStream.next()`:
// the batch string is a JSON array of these objects.
class SseEvent {
    event: string?,
    data: string?,
    id: string?,
}

// One `SseStream.next()` batch -> the raw SSE events it contains. Provider
// decoders parse each event's `data` payload with their own envelope classes.
function decode_sse_batch(batch: string) -> SseEvent[] {
    baml.json.from_string<SseEvent[]>(batch)
}

// The provider-agnostic events a decoder yields from one SSE batch.

// One incremental piece of assistant text. The delta contract: the chunk is
// NOT the accumulated text so far, it is only the newly arrived piece.
class TextDelta {
    text: string,
}

// Turn metadata as it arrives on the wire. Providers send stop_reason and
// token counts on different events; each TurnMeta carries what its event had
// and TurnStream folds non-null fields together (last write wins).
class TurnMeta {
    stop_reason: root.content.StopReason?,
    input_tokens: int?,
    output_tokens: int?,
}

// The provider's explicit end-of-turn marker — the decoded form of `[DONE]`,
// `response.completed`, `message_stop`, a chunk with `finishReason`, …
//
// A decoder for a protocol that HAS such a marker must yield TurnDone when it
// sees it AND its `from_sse` call must pass `require_terminal` (see the
// termination contract at the top of this file); the stream then reports a
// connection that closed early as a failure instead of a complete turn. For a
// protocol with no terminal marker, TurnDone is optional and a socket close
// still completes the turn.
class TurnDone {
}

type StreamEvent = TextDelta | TurnMeta | TurnDone;

// The single end-of-stream marker returned by both raw turn streams and typed
// function streams. A nominal sentinel is required because null may itself be
// a valid partial value.
class Done {
}

type EventSource = () -> StreamEvent | Done throws root.errors.InvokeError;

// The raw text-delta stream of ONE model turn. Pull-based: nothing is read
// from the wire until `next()` or `final_turn()` is called.
//
// `next()` yields DELTA chunks (each item is only the newly arrived text,
// never the accumulated prefix) and returns Done when the turn is over.
// `final_turn()` drains the rest of the stream and returns the completed
// turn: the accumulated text as one Text block, the folded stop_reason, and
// usage where the wire provided token counts.
class TurnStream {
    // callback-backed streams: composite clients use this to relay normalized
    // events from a selected member without exposing provider wire details.
    _event_source: EventSource?,
    // wire-backed streams: the SSE connection plus a per-provider decoder
    // that turns one `SseStream.next()` batch into StreamEvents.
    _sse: baml.http.SseStream?,
    _decode: ((string) -> StreamEvent[] throws root.errors.InvokeError)?,
    // scripted streams (tests): literal delta chunks, no network.
    _chunks: string[],
    _cursor: int,
    // decoded events not yet delivered.
    _pending: StreamEvent[],
    _pcursor: int,
    // folded state.
    _text: string,
    _stop_reason: root.content.StopReason?,
    _input_tokens: int?,
    _output_tokens: int?,
    _done: bool,
    // strict termination (see the contract at the top of this file): the
    // provider id when this stream requires an explicit TurnDone, null when a
    // socket close is allowed to complete the turn.
    _require_terminal: string?,
    // whether a TurnDone was actually decoded before the stream ended.
    _saw_terminal: bool,

    // A turn stream over a live SSE connection. `decode` receives the raw
    // batch string from `sse.next()` (a JSON array of SseEvent objects; see
    // decode_sse_batch) and yields the events it contains.
    //
    // `require_terminal` opts into the strict termination contract: pass the
    // provider id (`"anthropic"`, `"openai"`, …) when the protocol has an
    // explicit end-of-turn event that the decoder yields as `TurnDone`. The
    // stream then throws `ai.errors.NetworkFailure` if the connection ends
    // before that event arrives, rather than reporting the truncated text as a
    // completed turn. Leave it null (the default) only for a protocol whose
    // sole end-of-turn signal IS the socket closing.
    function from_sse(
        sse: baml.http.SseStream,
        decode: (string) -> StreamEvent[] throws root.errors.InvokeError,
        require_terminal: string? = null,
    ) -> TurnStream {
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
            _done: false,
            _require_terminal: require_terminal,
            _saw_terminal: false,
        }
    }

    // A turn stream fed from literal delta chunks — the scripted counterpart
    // of a live stream, for tests. There is no wire to be cut, so a scripted
    // stream is never strict: the turn completes as StopReason.Complete after
    // the last chunk.
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
            _done: false,
            _require_terminal: null,
            _saw_terminal: false,
        }
    }

    // A stream backed by one normalized-event callback. Reliability clients
    // use this to relay a chosen member's deltas and final metadata while
    // retaining TurnStream's ordinary accumulation and termination behavior.
    // The source itself owns any retry/fallback policy; once it returns a
    // TextDelta, that delta is immediately visible to the caller.
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
            _done: false,
            _require_terminal: null,
            _saw_terminal: false,
        }
    }

    function _finish(self) -> void {
        self._done = true;
        if let sse: baml.http.SseStream = self._sse {
            sse.close();
        }
    }

    // The strict-termination check: a no-op unless this stream required an
    // explicit terminal event and the stream ended without one, in which case
    // the truncated turn is reported as the transport failure it is instead of
    // being passed off as complete. Called on every end-of-stream path.
    function _check_terminal(self) -> void {
        if let provider: string = self._require_terminal {
            if (!self._saw_terminal) {
                let partial: string? = if (self._text == "") {
                    null
                } else {
                    self._text
                };
                throw root.errors.NetworkFailure {
                    provider: provider,
                    detail: `stream ended before the provider's terminal event: the connection closed after ${self._text.length()} characters of assistant text with no end-of-turn event, so the turn is truncated`,
                    raw_body: partial,
                }
            }
        }
    }

    // The next text DELTA, or Done when the turn is over. Metadata events are
    // folded silently; calling next() after Done keeps returning Done.
    function next(self) -> string | Done {
        while (true) {
            if (self._done) {
                return Done {  };
            }
            // deliver decoded events before pulling more from the wire
            if (self._pcursor < self._pending.length()) {
                let e = self._pending.at(self._pcursor);
                self._pcursor += 1;
                match (e) {
                    let d: TextDelta => {
                        self._text = `${self._text}${d.text}`;
                        return d.text;
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
                        null
                    },
                    let d: TurnDone => {
                        self._saw_terminal = true;
                        self._finish();
                        return Done {  };
                    },
                    null => null,
                };
                continue;
            }
            if let source: EventSource = self._event_source {
                match (source()) {
                    let event: StreamEvent => {
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
                    match (sse.next()) {
                        null => {
                            // end of connection. On a strict stream with no
                            // TurnDone decoded this is a truncated turn, and
                            // _check_terminal throws rather than completing it.
                            self._finish();
                            self._check_terminal();
                            return Done {  };
                        },
                        let batch: string => {
                            if let decode: (string) -> StreamEvent[] throws root.errors.InvokeError = self._decode {
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
                            self._cursor += 1;
                            self._text = `${self._text}${c}`;
                            return c;
                        },
                        null => {
                            self._finish();
                            return Done {  };
                        },
                    }
                },
            };
        }
        baml.sys.panic("unreachable")
    }

    // Drain the stream and return the completed turn. Safe to call at any
    // point (including after Done); calling twice returns an equal turn.
    //
    // On a strict stream (from_sse's `require_terminal`) that ended without the
    // provider's terminal event this throws ai.errors.NetworkFailure instead of
    // returning a turn — the stop_reason default below must never turn a
    // truncated stream into a `StopReason.Complete` answer.
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
                cached_input_tokens: null,
                reasoning_tokens: null,
            }
        } else {
            null
        };
        root.ModelTurn {
            content: [root.content.Text { text: self._text }],
            stop_reason: self._stop_reason ?? root.content.StopReason.Complete,
            usage: usage,
        }
    }
}

// The typed partial-value stream returned by an LLM function's `$stream`
// companion. This is the one canonical host-stream class; tagged handles carry
// its FQN across FFI so host bridges do not need to hardcode a namespace.
class Stream<TStream, TFinal> {
    _next_text: () -> string? throws root.errors.InvokeError,
    _cache: baml.sap.ParseCache<TStream, TFinal>,
    _text: string,
    _finished: bool,

    /// Return the next parsed partial value, or Done after EOF.
    function next(self) -> TStream | Done {
        while (true) {
            if (self._finished) {
                return Done {  };
            }
            match (self._next_text()) {
                null => {
                    self._finished = true;
                    return Done {  };
                },
                let s: string => {
                    self._text = self._text + s;
                    let parsed: TStream | baml.sap.NoYield = baml.sap.__parse_partial<TStream, TFinal>(self._text, self._cache);
                    match (parsed) {
                        baml.sap.NoYield => {},
                        let p: TStream => {
                            return p;
                        },
                    }
                },
            }
        }
        baml.sys.panic("unreachable")
    }

    /// Drain the stream and parse the accumulated text as the final value.
    function final(self) -> TFinal {
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
        baml.sap.__parse_final<TStream, TFinal>(self._text, self._cache)
    }
}

// One-turn streaming over a spec: render the spec's prompt exactly as the
// default runner's first turn would, require a streaming-capable client, and
// return the typed partial stream. `Fn$stream(...)` desugars to
// `ai.stream.from_spec<Out$stream, Out>(Fn$spec(...), client)`.
//
// Streaming does not run the tool loop yet: a spec with a non-empty toolbox
// is rejected. The client override is typed as the capability, so passing a
// non-streaming client fails at the call site; the spec's DEFAULT client is
// only checked at runtime (root.errors.StreamingUnsupported when it cannot
// stream).
function from_spec<TStream, TFinal>(
    spec: root.FunctionSpec<TFinal>,
    client: StreamingClient? = null,
) -> Stream<TStream, TFinal> {
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
    let _ = provider;
    Stream {
        _next_text: () -> {
            match (turns.next()) {
                let d: Done => null,
                let s: string => s,
            }
        },
        _cache: baml.sap.__new_parse_cache<TStream, TFinal>(),
        _text: "",
        _finished: false,
    }
}
