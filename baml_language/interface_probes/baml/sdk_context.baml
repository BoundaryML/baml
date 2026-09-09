// Generated TypeScript context probe; ordinary copied records isolate codecs
// from the separate, unfinished concrete/interface wrapper generation work.
class CodecPayload {
    text: string,
}

function make_codec_payload() -> CodecPayload throws never {
    CodecPayload { text: "Ada" }
}

function copy_codec_payload(value: CodecPayload) -> CodecPayload throws never {
    value
}

function codec_closure() -> (value: CodecPayload) -> CodecPayload throws never {
    (value) -> { value }
}

function apply_codec_closure(callback: (CodecPayload) -> CodecPayload throws never, value: CodecPayload) -> CodecPayload throws never {
    callback(value)
}

function visit_codec(callback: (CodecPayload) -> CodecPayload throws never) -> CodecPayload throws never {
    callback(make_codec_payload())
}
