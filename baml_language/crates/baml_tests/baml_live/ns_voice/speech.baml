// Test support: text to speech and speech to text over OpenAI's HTTP audio
// endpoints. A scripted conversation synthesizes the caller's lines with
// `synthesize` and checks what the assistant said with `transcribe`.

/// Speak `text`. Returns PCM16, 24 kHz, mono, which is the session wire format.
function synthesize(
    text: string,
    voice: string = "alloy",
    model: string = "gpt-4o-mini-tts",
    api_key: ai.Credential? = null,
) -> uint8array {
    let key = _openai_key(api_key);
    let response = baml.http.send(
        baml.http.Request {
            method: "POST",
            url: "https://api.openai.com/v1/audio/speech",
            headers: { "authorization": `Bearer ${key}`, "content-type": "application/json" },
            body: baml.json.stringify(
                { "model": model, "voice": voice, "input": text, "response_format": "pcm" },
            ),
        },
        timeout = baml.time.Duration.from_seconds(60),
    );
    if (!response.ok()) {
        throw baml.errors.Io { message: `text to speech failed with ${response.status_code}: ${response.text()}` }
    }
    response.bytes()
}

/// Transcribe `pcm`. The transcription endpoint takes a multipart upload,
/// which `baml.http` cannot build from a string body, so this writes a WAV
/// file and sends it with curl. The key travels in a curl config on stdin and
/// never appears in the process arguments.
function transcribe(
    pcm: uint8array,
    model: string = "gpt-4o-mini-transcribe",
    api_key: ai.Credential? = null,
) -> string {
    let key = _openai_key(api_key);
    let path = `${scratch_dir()}/transcribe_${pcm.length()}.wav`;
    let _ = baml.fs.write_bytes(path, wav(pcm));
    let result = baml.sys.exec(
        "curl",
        [
            "--silent",
            "--show-error",
            "--config",
            "-",
            "https://api.openai.com/v1/audio/transcriptions",
            "--form",
            `file=@${path}`,
            "--form",
            `model=${model}`,
            "--form",
            "response_format=json",
        ],
        baml.sys.ProcessOptions {
            cwd: null,
            env: null,
            timeout_ms: 120000,
            stdin: `header = "Authorization: Bearer ${key}"\n`,
            stderr: null,
        },
    );
    let _ = baml.fs.remove(path) catch_all (e) {
        _ => null,
    };
    let body = result.stdout.to_string();
    if (!result.ok()) {
        throw baml.errors.Io { message: `transcription failed: ${result.stderr.to_string()} ${body}` }
    }
    let parsed = baml.json.parse(body) catch_all (e) {
        _ => null,
    };
    baml.json.path_or<string>(parsed, ".text", "")
}

/// A directory for recordings and temporary files.
function scratch_dir() -> string {
    let tmp = (
        baml.env.get("TMPDIR") catch_all (e) {
            _ => null,
        }
    )
        ?? "/tmp";
    let dir = if (tmp.ends_with("/")) {
        tmp + "baml_live"
    } else {
        tmp + "/baml_live"
    };
    if (!baml.fs.exists(dir)) {
        baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true });
    }
    dir
}
