class TranscriptionOptions {
  language: string?,
  prompt: string?,
}

interface TranscriptionProvider requires Provider {
  function transcribe(self, stream: AudioStream, options: TranscriptionOptions) -> Response<string>
    throws baml.errors.CallError | baml.errors.UnknownError
}

class FakeTranscriptionProvider {
  output: string,
  received_chunks: audio[],
}

implements Provider for FakeTranscriptionProvider {
  function name(self) -> string throws never { "fake-transcription" }
}

implements TranscriptionProvider for FakeTranscriptionProvider {
  function transcribe(self, stream: AudioStream, options: TranscriptionOptions) -> Response<string>
      throws never {
    self.received_chunks = stream.collect();
    Response<string> {
      value: self.output,
      meta: Meta {
        provider: self.name(),
        model: "fake",
        request_id: null,
        finish_reason: "stop",
        usage: null,
        attributes: {
          "chunks": self.received_chunks.length(),
          "sample_rate_hz": stream.sample_rate_hz,
        },
        raw: null,
      },
      transcript: null,
    }
  }
}

// A finite-audio adapter over OpenAI's multimodal generation capability. The
// current guide surface intentionally accepts one bounded audio item per call;
// realtime/multi-turn input belongs to RealtimeProvider instead.
class OpenAiAudioTranscription {
  inner: OpenAi,
  received_chunks: int,
  implements Provider {
    function name(self) -> string throws never { "openai-audio-transcription" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }
  implements TranscriptionProvider {
    function transcribe(self, stream: AudioStream, options: TranscriptionOptions) -> Response<string>
        throws baml.errors.UnknownError {
      let chunks = stream.collect();
      if (chunks.length() != 1) {
        throw baml.errors.UnknownError {
          data: chunks.length(),
          message: ["bounded transcription currently requires exactly one audio chunk"],
        };
      }
      self.received_chunks = chunks.length();
      let chunk = chunks[0];
      let language = options.language ?? "auto-detect";
      let hint = options.prompt ?? "none";
      let task = task_named<string>(
        self.inner,
        "TranscribeBoundedAudio",
        {
          "sample_rate_hz": stream.sample_rate_hz,
          "channels": stream.channels,
          "language": language,
        },
        [],
        TaskOptions {
          tool_registry: null,
          dispatch: null,
          hooks: null,
          observers: [],
          recorders: [],
          run_id: null,
        },
        (ctx: baml.llm.Context) -> baml.llm.PromptAst {
          baml.llm.assemble_prompt_ast(
            [
              `Transcribe the spoken words exactly. Language: ${language}. Context hint: ${hint}.\nAudio: `,
              "\nReturn only the transcript.\n",
              "",
            ],
            [chunk, ctx.output_format],
          )
        },
      );
      let response = self.inner.generate<string>(task);
      Response<string> {
        ...response,
        meta: Meta {
          ...response.meta,
          attributes: {
            "capability": "bounded_transcription",
            "chunks": self.received_chunks,
            "sample_rate_hz": stream.sample_rate_hz,
            "channels": stream.channels,
          },
        },
      }
    }
  }
}
