// Static Python SDK coverage for every place a stream type can appear.
type TextResultStream = ai.stream.Stream<string, string>;

class TextResultStreamHolder {
    stream: ai.stream.Stream<string, string>,
    aliased: TextResultStream,
}

class TextResultStreamMethods {
    function echo(
        self,
        stream: ai.stream.Stream<string, string>,
    ) -> ai.stream.Stream<string, string> {
        stream
    }

    function echo_static(
        stream: ai.stream.Stream<string, string>,
    ) -> ai.stream.Stream<string, string> {
        stream
    }
}

function accept_text_stream(
    stream: ai.stream.Stream<string, string>,
) -> ai.stream.Stream<string, string> {
    stream
}

function accept_aliased_text_stream(stream: TextResultStream) -> TextResultStream {
    stream
}

function wrap_text_stream(
    stream: ai.stream.Stream<string, string>,
) -> ai.stream.Stream<string, string>[] {
    [stream]
}

function maybe_text_stream(
    stream: ai.stream.Stream<string, string>,
    present: bool,
) -> ai.stream.Stream<string, string>? {
    if (present) {
        stream
    } else {
        null
    }
}

function call_text_stream_callback(
    callback: (ai.stream.Stream<string, string>) -> ai.stream.Stream<string, string>,
    stream: ai.stream.Stream<string, string>,
) -> ai.stream.Stream<string, string> {
    callback(stream)
}
