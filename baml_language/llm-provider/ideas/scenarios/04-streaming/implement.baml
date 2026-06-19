// ============================================================================
// implement.baml — Streaming tokens + streaming partial structured output
//
// Library / std-lib-author code. Builds on the canonical primitives from
// ../_conventions.md (Provider, HttpProvider, Streaming, baml.llm.Stream).
//
// What this file has to show absorbing:
//   - ONE Streaming interface over TWO wire formats: SSE (OpenAI/Anthropic) and
//     Gemini chunked-JSON. Two stream readers behind one method.
//   - baml.llm.Stream<TStream, TFinal>: TStream is the per-step partial,
//     TFinal is the validated answer. The same stream feeds tokens AND
//     partial structured output — partial parsing is a *client-side* read.
//   - Reassembly is STATEFUL: a mutable accumulator the provider owns while
//     it drains the wire.
//   - Fallback-to-call: a non-streaming provider degrades via buffer_as_stream.
// ============================================================================


// ----------------------------------------------------------------------------
// 0. Net-new host primitives this scenario needs.
//
// baml.llm.Stream<TStream, TFinal> already exists in the conventions (it is
// listed as reused machinery). We pin down its surface here because the whole
// scenario is about *what you can do with it*. It is a pull-based async
// iterator (mirror of ns_iter's Iterator), with a terminal `final()`:
//
//   interface Stream<TStream, TFinal> {
//     function next(self)  -> TStream | StreamDone throws baml.errors.Io   // typewriter step
//     function final(self) -> TFinal throws baml.errors.LlmClient          // validated answer
//     function close(self) -> null                                          // cancel mid-stream
//   }
//
// `StreamDone` is the sentinel (like Iterator's Done). A consumer loops `next`
// for incremental UI, then calls `final` for the normal message.
//
// We do NOT reimplement Stream — we *construct* values of it. The two
// constructors below are the net-new host surface (flagged in evaluation.md):
//
//   baml.llm.stream_unfold<S, TStream, TFinal>(
//       seed:  S,                                   // the stateful accumulator
//       step:  (S) -> StreamStep<S, TStream>,       // pull one wire frame, fold it in
//       done:  (S) -> TFinal,                       // build the final object from the accumulator
//   ) -> baml.llm.Stream<TStream, TFinal>
//
//   baml.llm.stream_once<TStream, TFinal>(
//       partial: TStream,                           // a single already-complete partial
//       final:   TFinal,                            // the validated answer
//   ) -> baml.llm.Stream<TStream, TFinal>           // used by the fallback path
//
// `stream_unfold` is the load-bearing one: it turns a stateful reassembly
// closure into a Stream without the provider author hand-writing an iterator
// class. `StreamStep` is the per-pull result.
// ----------------------------------------------------------------------------

class StreamStep<S, TStream> {
  state: S                  // the folded-forward accumulator
  emit: TStream?            // a partial to surface this step (null => internal frame, skip)
  done: bool                // true => stop pulling; `done(state)` builds TFinal
}


// ----------------------------------------------------------------------------
// 1. The stateful accumulator.
//
// "Reassembly is stateful" — this class IS that state. It is provider-agnostic:
// both the SSE reader and the Gemini chunked-JSON reader fold their frames into
// the same shape. `buffer` is the raw concatenated model text (the JSON-so-far
// for structured output, or just prose for token streaming). The other fields
// capture the divergent finish/usage signals.
// ----------------------------------------------------------------------------

class StreamAcc {
  buffer: string            // concatenated text deltas (the growing answer / growing JSON)
  thinking: string          // Anthropic thinking_delta — kept SEPARATE, never mixed into buffer
  finish_reason: string?    // "stop" | "length" | "tool_calls" | provider-specific
  prompt_tokens: int?       // Anthropic delivers this at message_start; OpenAI at the end
  output_tokens: int?
  open_block: int?          // Anthropic content-block index currently streaming (interleave guard)

  function empty() -> StreamAcc {                 // static (no self)
    StreamAcc {
      buffer: "", thinking: "", finish_reason: null,
      prompt_tokens: null, output_tokens: null, open_block: null,
    }
  }
}


// ----------------------------------------------------------------------------
// 2. The partial-parse seam.
//
// TStream is whatever the *consumer* asked for. For a typewriter UI it is
// `string` (raw delta). For streaming structured output it is a deep-partial of
// the target schema. Both come off the SAME buffer; the only difference is how
// you read it. We expose ONE host hook for tolerant partial parsing:
//
//   baml.sap.parse_partial<TStream>(buffer: string) -> TStream throws baml.errors.LlmClient
//
// This is the streaming twin of baml.sap.parse<T> (conventions, §SAP): it
// returns the best-effort deep-partial of the growing buffer, treating
// unterminated strings / missing closers as "still streaming". For TStream =
// string it just returns the buffer. The provider does NOT know or care which
// TStream the consumer chose — it is a generic over the method.
// ----------------------------------------------------------------------------


// ----------------------------------------------------------------------------
// 3. OpenAI — SSE transport, flat deltas.
//
// HttpProvider for the plain .call path (Body = Response, SAP the whole text),
// PLUS Streaming for incremental delivery. The streaming `send` opens an SSE
// stream; reassembly folds `choices[0].delta.content` into buffer.
// ----------------------------------------------------------------------------

class OpenAI {
  model: string
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response

    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      // For non-stream .call: response_format = json_schema of T. (One of the four
      // encodings; the streaming build_request below flips `stream: true`.)
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "openai http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text() catch (e) { _ => { throw baml.errors.LlmClient { message: "no body" }; } })
    }
  }

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient {
      // 1. Build the SAME request as .call, but with stream:true and (for OpenAI)
      //    stream_options.include_usage so the final chunk carries usage.
      let req: baml.http.Request = self.build_request<TFinal>(prompt);
      let stream_req: baml.http.Request = baml.llm.openai_mark_stream(req);   // sets stream:true + include_usage

      // 2. Open the SSE reader. This is the OpenAI/Anthropic wire primitive.
      let sse: baml.http.SseStream =
        baml.http.fetch_sse(stream_req)
          catch (e) { _ => { throw baml.errors.LlmClient { message: "openai sse failed" }; } };

      // 3. Unfold the SSE stream into a baml.llm.Stream. The closure is the
      //    stateful reassembly: pull one `data:` line, fold its delta into buffer,
      //    re-parse the partial. `(StreamAcc, SseStream)` is the seed pair.
      baml.llm.stream_unfold<(StreamAcc, baml.http.SseStream), TStream, TFinal>(
        (StreamAcc.empty(), sse),
        seed => { OpenAI.fold_sse<TStream>(seed) },          // step
        seed => { OpenAI.finalize<TFinal>(seed) },           // done -> validated TFinal
      )
    }
  }

  // ----- stateful SSE reassembly for OpenAI's flat deltas -----
  function fold_sse<TStream>(seed: (StreamAcc, baml.http.SseStream))
      -> StreamStep<(StreamAcc, baml.http.SseStream), TStream> throws baml.errors.LlmClient {
    let acc: StreamAcc = seed.0;
    let sse: baml.http.SseStream = seed.1;

    let line: string? = sse.next() catch (e) { _ => { throw baml.errors.LlmClient { message: "sse read" }; } };
    match (line) {
      null => {                                              // socket closed without [DONE]
        return StreamStep<(StreamAcc, baml.http.SseStream), TStream> { state: (acc, sse), emit: null, done: true };
      }
      let raw: string => {
        if (raw == "[DONE]") {                               // OpenAI's literal terminal sentinel
          return StreamStep<(StreamAcc, baml.http.SseStream), TStream> { state: (acc, sse), emit: null, done: true };
        }
        // Each data line is a JSON chunk. Pull delta + finish_reason + (final) usage.
        let delta: string? = baml.json.get_str(raw, "choices.0.delta.content");
        let finish: string? = baml.json.get_str(raw, "choices.0.finish_reason");
        let ptoks: int? = baml.json.get_int(raw, "usage.prompt_tokens");
        let otoks: int? = baml.json.get_int(raw, "usage.completion_tokens");

        let next_acc: StreamAcc = StreamAcc {
          buffer: acc.buffer + (delta ?? ""),
          thinking: acc.thinking,
          finish_reason: finish ?? acc.finish_reason,
          prompt_tokens: ptoks ?? acc.prompt_tokens,
          output_tokens: otoks ?? acc.output_tokens,
          open_block: acc.open_block,
        };

        // No new text this frame (usage-only / role-only chunk) => internal, skip emit.
        match (delta) {
          null => {
            return StreamStep<(StreamAcc, baml.http.SseStream), TStream> {
              state: (next_acc, sse), emit: null, done: false,
            };
          }
          _ => {
            // Re-parse the WHOLE growing buffer into the consumer's TStream.
            // For TStream=string this is the buffer; for a deep-partial it's the
            // tolerant SAP parse. Provider is generic over which.
            let partial: TStream = baml.sap.parse_partial<TStream>(next_acc.buffer);
            return StreamStep<(StreamAcc, baml.http.SseStream), TStream> {
              state: (next_acc, sse), emit: partial, done: false,
            };
          }
        }
      }
    }
  }

  // ----- build the validated final object from the drained buffer -----
  function finalize<TFinal>(seed: (StreamAcc, baml.http.SseStream)) -> TFinal throws baml.errors.LlmClient {
    let acc: StreamAcc = seed.0;
    seed.1.close();                                          // release the socket
    // The "final object still has to be built": full SAP validation (required
    // fields, coercion) on the complete buffer — NOT the tolerant partial parse.
    baml.sap.parse<TFinal>(acc.buffer)
  }
}


// ----------------------------------------------------------------------------
// 4. Anthropic — SSE transport too, but a STRUCTURED event grammar.
//
// Same Streaming interface, same StreamAcc, same partial seam — but the fold
// is different: typed events (message_start / content_block_delta / message_delta),
// content-block indices, thinking_delta kept out of the answer, usage split
// across start+end. This is the "two stream readers" point made concrete even
// *within* SSE: OpenAI's flat deltas and Anthropic's event grammar need
// different fold logic behind the one method.
// ----------------------------------------------------------------------------

class Anthropic {
  model: string
  api_key: string?
  max_tokens: int                                           // bespoke field (required by Anthropic)

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function                                      // Anthropic forced-tool encoding of T
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "anthropic http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text() catch (e) { _ => { throw baml.errors.LlmClient { message: "no body" }; } })
    }
  }

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient {
      let req: baml.http.Request = self.build_request<TFinal>(prompt);
      let stream_req: baml.http.Request = baml.llm.anthropic_mark_stream(req);   // sets "stream": true
      let sse: baml.http.SseStream =
        baml.http.fetch_sse(stream_req)
          catch (e) { _ => { throw baml.errors.LlmClient { message: "anthropic sse failed" }; } };

      baml.llm.stream_unfold<(StreamAcc, baml.http.SseStream), TStream, TFinal>(
        (StreamAcc.empty(), sse),
        seed => { Anthropic.fold_events<TStream>(seed) },
        seed => { Anthropic.finalize<TFinal>(seed) },
      )
    }
  }

  // ----- stateful reassembly for Anthropic's event grammar -----
  function fold_events<TStream>(seed: (StreamAcc, baml.http.SseStream))
      -> StreamStep<(StreamAcc, baml.http.SseStream), TStream> throws baml.errors.LlmClient {
    let acc: StreamAcc = seed.0;
    let sse: baml.http.SseStream = seed.1;

    let line: string? = sse.next() catch (e) { _ => { throw baml.errors.LlmClient { message: "sse read" }; } };
    match (line) {
      null => { return StreamStep<(StreamAcc, baml.http.SseStream), TStream> { state: (acc, sse), emit: null, done: true }; }
      let raw: string => {
        let etype: string = baml.json.get_str(raw, "type") ?? "";

        if (etype == "message_start") {
          // input-token usage arrives here, at the START
          let acc2: StreamAcc = StreamAcc {
            buffer: acc.buffer, thinking: acc.thinking, finish_reason: acc.finish_reason,
            prompt_tokens: baml.json.get_int(raw, "message.usage.input_tokens") ?? acc.prompt_tokens,
            output_tokens: acc.output_tokens, open_block: acc.open_block,
          };
          return StreamStep<(StreamAcc, baml.http.SseStream), TStream> { state: (acc2, sse), emit: null, done: false };
        }
        if (etype == "content_block_start") {
          let acc2: StreamAcc = StreamAcc {
            buffer: acc.buffer, thinking: acc.thinking, finish_reason: acc.finish_reason,
            prompt_tokens: acc.prompt_tokens, output_tokens: acc.output_tokens,
            open_block: baml.json.get_int(raw, "index"),    // track which block is streaming
          };
          return StreamStep<(StreamAcc, baml.http.SseStream), TStream> { state: (acc2, sse), emit: null, done: false };
        }
        if (etype == "content_block_delta") {
          let dtype: string = baml.json.get_str(raw, "delta.type") ?? "";
          if (dtype == "thinking_delta") {
            // thinking deltas must NOT be mixed into the answer buffer
            let acc2: StreamAcc = StreamAcc {
              buffer: acc.buffer,
              thinking: acc.thinking + (baml.json.get_str(raw, "delta.thinking") ?? ""),
              finish_reason: acc.finish_reason, prompt_tokens: acc.prompt_tokens,
              output_tokens: acc.output_tokens, open_block: acc.open_block,
            };
            return StreamStep<(StreamAcc, baml.http.SseStream), TStream> { state: (acc2, sse), emit: null, done: false };
          }
          // text_delta (or input_json_delta for forced-tool structured output) -> answer buffer
          let frag: string =
            baml.json.get_str(raw, "delta.text")
            ?? baml.json.get_str(raw, "delta.partial_json")
            ?? "";
          let acc2: StreamAcc = StreamAcc {
            buffer: acc.buffer + frag, thinking: acc.thinking,
            finish_reason: acc.finish_reason, prompt_tokens: acc.prompt_tokens,
            output_tokens: acc.output_tokens, open_block: acc.open_block,
          };
          let partial: TStream = baml.sap.parse_partial<TStream>(acc2.buffer);
          return StreamStep<(StreamAcc, baml.http.SseStream), TStream> { state: (acc2, sse), emit: partial, done: false };
        }
        if (etype == "message_delta") {
          // stop reason + output-token usage at the END
          let acc2: StreamAcc = StreamAcc {
            buffer: acc.buffer, thinking: acc.thinking,
            finish_reason: baml.json.get_str(raw, "delta.stop_reason") ?? acc.finish_reason,
            prompt_tokens: acc.prompt_tokens,
            output_tokens: baml.json.get_int(raw, "usage.output_tokens") ?? acc.output_tokens,
            open_block: acc.open_block,
          };
          return StreamStep<(StreamAcc, baml.http.SseStream), TStream> { state: (acc2, sse), emit: null, done: false };
        }
        if (etype == "message_stop") {
          return StreamStep<(StreamAcc, baml.http.SseStream), TStream> { state: (acc, sse), emit: null, done: true };
        }
        // content_block_stop / ping / unknown -> internal frame
        StreamStep<(StreamAcc, baml.http.SseStream), TStream> { state: (acc, sse), emit: null, done: false }
      }
    }
  }

  function finalize<TFinal>(seed: (StreamAcc, baml.http.SseStream)) -> TFinal throws baml.errors.LlmClient {
    seed.1.close();
    baml.sap.parse<TFinal>(seed.0.buffer)
  }
}


// ----------------------------------------------------------------------------
// 5. Gemini — DIFFERENT TRANSPORT. Not SSE: chunked JSON.
//
// streamGenerateContent returns ONE HTTP response whose body is a JSON array of
// full GenerateContentResponse objects delivered incrementally. fetch_sse cannot
// read this — "open an SSE stream" is not a universal primitive. We need a second
// host reader for incremental JSON-array elements:
//
//   baml.http.fetch_json_seq(request) -> JsonSeq throws baml.errors.Io
//   class JsonSeq { url: string, _handle: $rust_type }  // .next() -> string? throws Io  (one array element); .close()
//
// This is NET-NEW host surface (flagged in evaluation.md): the conventions only
// ship fetch_sse. The Streaming *interface* is unchanged — only the reader behind
// it differs. Also note: each Gemini chunk is a COMPLETE object whose parts[0].text
// is that chunk's increment, so the fold still concatenates into buffer.
// ----------------------------------------------------------------------------

class Gemini {
  model: string
  api_key: string

  implements ChatModelOptions { model as model, api_key as api_key }

  implements HttpProvider {
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function                                      // Gemini responseSchema encoding of T
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "gemini http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text() catch (e) { _ => { throw baml.errors.LlmClient { message: "no body" }; } })
    }
  }

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient {
      let req: baml.http.Request = self.build_request<TFinal>(prompt);
      // route to :streamGenerateContent (vs :generateContent) — a URL change, not a body flag
      let stream_req: baml.http.Request = baml.llm.gemini_mark_stream(req);
      let seq: baml.http.JsonSeq =
        baml.http.fetch_json_seq(stream_req)               // SECOND reader: chunked JSON, not SSE
          catch (e) { _ => { throw baml.errors.LlmClient { message: "gemini stream failed" }; } };

      baml.llm.stream_unfold<(StreamAcc, baml.http.JsonSeq), TStream, TFinal>(
        (StreamAcc.empty(), seq),
        seed => { Gemini.fold_chunks<TStream>(seed) },
        seed => { Gemini.finalize<TFinal>(seed) },
      )
    }
  }

  // ----- stateful reassembly over JSON-array elements -----
  function fold_chunks<TStream>(seed: (StreamAcc, baml.http.JsonSeq))
      -> StreamStep<(StreamAcc, baml.http.JsonSeq), TStream> throws baml.errors.LlmClient {
    let acc: StreamAcc = seed.0;
    let seq: baml.http.JsonSeq = seed.1;

    let elem: string? = seq.next() catch (e) { _ => { throw baml.errors.LlmClient { message: "json seq read" }; } };
    match (elem) {
      null => { return StreamStep<(StreamAcc, baml.http.JsonSeq), TStream> { state: (acc, seq), emit: null, done: true }; }
      let raw: string => {
        // each `raw` is a COMPLETE GenerateContentResponse; its increment is parts[0].text
        let frag: string = baml.json.get_str(raw, "candidates.0.content.parts.0.text") ?? "";
        let finish: string? = baml.json.get_str(raw, "candidates.0.finishReason");
        let ptoks: int? = baml.json.get_int(raw, "usageMetadata.promptTokenCount");
        let otoks: int? = baml.json.get_int(raw, "usageMetadata.candidatesTokenCount");

        let acc2: StreamAcc = StreamAcc {
          buffer: acc.buffer + frag, thinking: acc.thinking,
          finish_reason: finish ?? acc.finish_reason,
          prompt_tokens: ptoks ?? acc.prompt_tokens,
          output_tokens: otoks ?? acc.output_tokens,
          open_block: acc.open_block,
        };
        if (frag == "") {
          return StreamStep<(StreamAcc, baml.http.JsonSeq), TStream> { state: (acc2, seq), emit: null, done: false };
        }
        let partial: TStream = baml.sap.parse_partial<TStream>(acc2.buffer);
        StreamStep<(StreamAcc, baml.http.JsonSeq), TStream> { state: (acc2, seq), emit: partial, done: false }
      }
    }
  }

  function finalize<TFinal>(seed: (StreamAcc, baml.http.JsonSeq)) -> TFinal throws baml.errors.LlmClient {
    seed.1.close();
    baml.sap.parse<TFinal>(seed.0.buffer)
  }
}


// ----------------------------------------------------------------------------
// 6. A NON-streaming provider, to exercise the fallback path.
//
// Some providers (a local toy model, an embeddings-only endpoint, a harness)
// implement only Provider.call — no Streaming. The .stream companion must still
// "work" by buffering the whole answer and emitting it as a one-shot stream.
// ----------------------------------------------------------------------------

class EchoModel {
  reply: string
  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(self.reply)
    }
  }
  // NOTE: no `implements Streaming`. This is the fallback case.
}


// ----------------------------------------------------------------------------
// 7. The fallback primitive used by the .stream companion's `_` arm.
//
// "buffer_as_stream": run the plain .call, then present the single complete
// answer as a degenerate stream — one partial (the whole thing) then final.
// This is the honest degrade for a delivery-refinement capability: the consumer
// still gets a TStream then a TFinal, just not incrementally.
//
// Subtlety the proposal must confront (see evaluation.md): the consumer asked
// for `baml.llm.Stream<TStream, TFinal>` with TWO independent type params, but
// .call only produces a TFinal. To emit a TStream we must derive the partial
// from the final — which only works when TStream is reachable from TFinal
// (string, or a deep-partial of TFinal). We require a host coercion:
//
//   baml.sap.partial_of<TStream, TFinal>(final: TFinal) -> TStream
//
// (the "everything optional, all present" deep-partial of an already-complete
// value). For TStream = string this stringifies; for TStream = Partial<TFinal>
// it widens. If TStream is unrelated to TFinal, this throws — the fallback can't
// invent a delivery shape it was never given a stream for.
// ----------------------------------------------------------------------------

function buffer_as_stream<TStream, TFinal>(p: Provider, prompt: baml.llm.PromptAst)
    -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient {
  let final: TFinal = p.call<TFinal>(prompt);
  let whole: TStream = baml.sap.partial_of<TStream, TFinal>(final);
  baml.llm.stream_once<TStream, TFinal>(whole, final)
}
