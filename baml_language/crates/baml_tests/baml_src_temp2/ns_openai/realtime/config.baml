class Realtime {
    model: string,
    voice: string,
    api_key: string,
    // null/false is text-only. true requests audio plus its provider transcript.
    output_audio: bool?,
    // Optional asynchronous input ASR. For example: gpt-4o-mini-transcribe.
    input_transcription_model: string?,
    // null means caller-delimited turns. A value enables provider-side speech
    // detection; its policy flags decide whether responses and interruption
    // remain automatic or are owned by the runner.
    server_vad: RealtimeServerVad?,
    // Keep the socket open after an ordinary response so callers can send turns.
    keep_open: bool?,
    implements root.ai.Provider {
        function name(self) -> string throws never {
            "openai-realtime"
        }
        function render_shorthand(self) -> string throws never {
            "openai/" + self.model
        }
    }
}

class RealtimeServerVad {
    threshold: float?,
    prefix_padding_ms: int?,
    silence_duration_ms: int?,
    // These default to OpenAI's ordinary hands-free conversation behavior.
    // Set interrupt_response=false when a runner owns a delayed barge-in policy.
    create_response: bool?,
    interrupt_response: bool?,
}

// OpenAI Realtime currently exchanges signed little-endian PCM16, mono,
// at 24 kHz. The provider package owns this requirement; portable ai runners
// only compare the session and device declarations.
function realtime_audio_format() -> root.ai.realtime.RealtimeAudioFormat throws never {
    root.ai.realtime.RealtimeAudioFormat {
        encoding: "pcm_s16le",
        sample_rate_hz: 24_000,
        channels: 1,
    }
}
