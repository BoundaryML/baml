enum JmesPathErrorKind {
    Syntax,
    InvalidArity,
    InvalidType,
    InvalidValue,
    UnknownFunction,
}

class JmesPathSuccess {
    value: baml.json.json,
}

class JmesPathFailure {
    kind: JmesPathErrorKind,
    message: string,
}

type JmesPathResult = JmesPathSuccess | JmesPathFailure;

class InlineJsonSource {
    value: baml.json.json,
}

class FileJsonSource {
    path: string,
}

class HttpJsonSource {
    url: string,
    headers: map<string, string>,
}

type JsonSource = InlineJsonSource | FileJsonSource | HttpJsonSource;

function jmespath_search(
    expression: string,
    source: JsonSource,
) -> JmesPathResult throws baml.errors.Io | baml.errors.Timeout | baml.json.JsonParseError {
    baml.sys.panic("not implemented")
}
