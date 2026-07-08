// scenario 25 — voice pipelines: ../../../../../llm-provider/ideas/scenarios/25-voice-pipelines/README.md
// STT + LLM + TTS as three single-capability stages wired together.

interface Stt {
    function transcribe(self, audio_url: string) -> string throws never
}

interface Tts {
    function synthesize(self, text: string) -> string throws never
}

class FakeStt {
    implements Stt {
        function transcribe(self, audio_url: string) -> string throws never {
            "transcript of " + audio_url
        }
    }
}

class FakeTts {
    implements Tts {
        function synthesize(self, text: string) -> string throws never {
            "audio<" + text + ">"
        }
    }
}

function cascaded_voice(
    stt: Stt,
    llm: baml.ai.Provider,
    tts: Tts,
    audio_url: string,
) -> string throws baml.errors.CallError | baml.errors.UnknownError {
    let text = stt.transcribe(audio_url);
    let reply = match (llm) {
        let h: baml.ai.HttpProvider => h.call<string>(text),
        _ => throw baml.errors.Unsupported { message: "llm stage cannot produce a value" },
    };
    tts.synthesize(reply)
}

test "25: the cascaded pipeline flows through all three stages" {
    let out = cascaded_voice(FakeStt {}, EchoProvider { reply: "llm reply" }, FakeTts {}, "call.wav") catch (e) {
        _ => "pipeline failed",
    };
    assert.equal(out, "audio<llm reply>")
}
