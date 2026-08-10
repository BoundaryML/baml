// Streaming: the raw text-delta stream of one model turn (TurnStream), the
// typed partial-value stream (Stream), and the one-turn streaming runner
// (stream_spec). Streaming is a client capability: a client that can stream
// implements StreamingClient; `Fn$stream` companions call stream_spec.
//
// Phase 1 scope: one model turn, no tool loop. A spec whose toolbox is not
// empty is rejected with baml.errors.InvalidArgument.

// A client that can render the same request as `invoke` but streamed.
// Implementors must also implement Client (`requires`).
interface StreamingClient requires Client {
    function invoke_stream(self, input: ModelTurnInput) -> TurnStream throws unknown
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
    stop_reason: StopReason?,
    input_tokens: int?,
    output_tokens: int?,
}

// The provider's explicit end-of-turn marker. Optional: a stream that simply
// ends (SSE close) also completes the turn.
class TurnDone {}

type StreamEvent = TextDelta | TurnMeta | TurnDone;

// The end-of-stream marker `TurnStream.next` returns.
class StreamDone {}

// The raw text-delta stream of ONE model turn. Pull-based: nothing is read
// from the wire until `next()` or `final_turn()` is called.
//
// `next()` yields DELTA chunks (each item is only the newly arrived text,
// never the accumulated prefix) and returns StreamDone when the turn is over.
// `final_turn()` drains the rest of the stream and returns the completed
// turn: the accumulated text as one Text block, the folded stop_reason, and
// usage where the wire provided token counts.
class TurnStream {
    // wire-backed streams: the SSE connection plus a per-provider decoder
    // that turns one `SseStream.next()` batch into StreamEvents.
    _sse: baml.http.SseStream?,
    _decode: ((string) -> StreamEvent[] throws unknown)?,
    // scripted streams (tests): literal delta chunks, no network.
    _chunks: string[],
    _cursor: int,
    // decoded events not yet delivered.
    _pending: StreamEvent[],
    _pcursor: int,
    // folded state.
    _text: string,
    _stop_reason: StopReason?,
    _input_tokens: int?,
    _output_tokens: int?,
    _done: bool,

    // A turn stream over a live SSE connection. `decode` receives the raw
    // batch string from `sse.next()` (a JSON array of SseEvent objects; see
    // decode_sse_batch) and yields the events it contains.
    function from_sse(
        sse: baml.http.SseStream,
        decode: (string) -> StreamEvent[] throws unknown,
    ) -> TurnStream {
        TurnStream {
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
        }
    }

    // A turn stream fed from literal delta chunks — the scripted counterpart
    // of a live stream, for tests. The turn completes as StopReason.Complete
    // after the last chunk.
    function from_chunks(chunks: string[]) -> TurnStream {
        TurnStream {
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
        }
    }

    function _finish(self) -> null {
        self._done = true;
        if let sse: baml.http.SseStream = self._sse {
            sse.close()
        } else {
            null
        }
    }

    // The next text DELTA, or StreamDone when the turn is over. Metadata
    // events are folded silently; calling next() after StreamDone keeps
    // returning StreamDone.
    function next(self) -> string | StreamDone {
        while (true) {
            if (self._done) {
                return StreamDone {};
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
                        if let s: StopReason = m.stop_reason {
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
                        let _ = self._finish();
                        return StreamDone {};
                    },
                    null => null,
                };
                continue;
            }
            match (self._sse) {
                let sse: baml.http.SseStream => {
                    match (sse.next()) {
                        null => {
                            let _ = self._finish();
                            return StreamDone {};
                        },
                        let batch: string => {
                            if let decode: (string) -> StreamEvent[] throws unknown = self._decode {
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
                            let _ = self._finish();
                            return StreamDone {};
                        },
                    }
                },
            };
        }
        baml.sys.panic("unreachable")
    }

    // Drain the stream and return the completed turn. Safe to call at any
    // point (including after StreamDone); calling twice returns an equal turn.
    function final_turn(self) -> ModelTurn {
        while (true) {
            match (self.next()) {
                let d: StreamDone => {
                    break;
                },
                _ => null,
            };
        }
        let usage: Usage? = if (self._input_tokens != null || self._output_tokens != null) {
            Usage {
                input_tokens: self._input_tokens ?? 0,
                output_tokens: self._output_tokens ?? 0,
                cached_input_tokens: null,
                reasoning_tokens: null,
            }
        } else {
            null
        };
        ModelTurn {
            content: [Text { text: self._text }],
            stop_reason: self._stop_reason ?? StopReason.Complete,
            usage: usage,
        }
    }
}

// The typed partial-value stream a `Fn$stream` companion returns.
//
// Pull-based, mirroring the legacy `baml.llm.Stream` semantics: `next()`
// returns the next partial value (SAP-parsing the accumulated text, skipping
// no-yield ticks) or `baml.stream.StreamFinished` when the turn is over;
// after StreamFinished, call `final()` for the final typed value. `final()`
// drains any remaining stream first, so it is valid at any point.
class Stream<TStream, TFinal> {
    _turns: TurnStream,
    _cache: baml.llm.StreamCache<TStream, TFinal>,
    _provider: string,
    _text: string,
    _finished: bool,

    // The next partial value, or StreamFinished when the stream is done.
    // Only returns partials: use final() for the final value.
    function next(self) -> TStream | baml.stream.StreamFinished {
        while (true) {
            if (self._finished) {
                return baml.stream.StreamFinished {};
            }
            match (self._turns.next()) {
                let d: StreamDone => {
                    self._finished = true;
                    return baml.stream.StreamFinished {};
                },
                let s: string => {
                    self._text = `${self._text}${s}`;
                    let parsed: TStream | baml.stream.StreamNoYield = baml.llm.__sap_parse_partial<TStream, TFinal>(self._text, self._cache);
                    match (parsed) {
                        baml.stream.StreamNoYield => null,
                        let p: TStream => {
                            return p;
                        },
                    };
                    null
                },
            };
        }
        baml.sys.panic("unreachable")
    }

    // The final typed value. Blocks until the stream is done; calling twice
    // returns the same value.
    function final(self) -> TFinal {
        let turn = self._turns.final_turn();
        self._finished = true;
        let candidate = turn.terminal_text() ?? "";
        match (turn.stop_reason) {
            StopReason.MaxTokens => {
                throw root.errors.ParseFailed { provider: self._provider, raw_output: candidate }
            },
            StopReason.Refused => {
                throw root.errors.Refused {
                    provider: self._provider,
                    reason: "the model declined to answer",
                    raw_body: null,
                }
            },
            _ => null,
        };
        baml.llm.__sap_parse_final<TStream, TFinal>(candidate, self._cache)
    }
}

// One-turn streaming over a spec: render the spec's prompt exactly as the
// default runner's first turn would, require a streaming-capable client, and
// return the typed partial stream. `Fn$stream(...)` desugars to
// `ai.stream_spec<Out$stream, Out>(Fn$spec(...), client)`.
//
// Streaming does not run the tool loop yet: a spec with a non-empty toolbox
// is rejected. The client override is typed as the capability, so passing a
// non-streaming client fails at the call site; the spec's DEFAULT client is
// only checked at runtime (root.errors.StreamingUnsupported when it cannot
// stream).
function stream_spec<TStream, TFinal>(
    spec: FunctionSpec<TFinal>,
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
        let c: Client => c.id(),
    };
    let turns = selected.invoke_stream(
        ModelTurnInput {
            prompt: spec.prompt(),
            journal: Journal.new(spec),
            toolbox: spec.tools(),
            output_type: spec.output_type(),
        },
    );
    Stream {
        _turns: turns,
        _cache: baml.llm.__new_stream_cache<TStream, TFinal>(),
        _provider: provider,
        _text: "",
        _finished: false,
    }
}
