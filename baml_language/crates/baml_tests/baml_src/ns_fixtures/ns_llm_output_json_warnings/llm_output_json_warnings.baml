// Warnings for the parts of an LLM function's output a model can never fill.
// A model answers with JSON: a return-type member with no JSON form is
// dropped from the output schema (E0184), and an optional field whose type
// has no JSON form is always null (E0185). The diagnostics snapshot pins
// both, and what does not warn.

class Blob {
    bytes: uint8array,
}

class Photo {
    caption: string,
    // E0185: the model can only leave it null.
    raw: uint8array?,
    // `@skip` says that is intended: no warning.
    thumbnail: uint8array? @skip,
    details: PhotoDetails,
}

class PhotoDetails {
    // E0185 in a class the output reaches through a field.
    exif: uint8array?,
    // A field the model can fill: no warning.
    location: string?,
}

type BytesOrText = uint8array | string;

// E0184: `Describe` can only return `string`.
function Describe(topic: string) -> uint8array | string {
    client: "openai/gpt-4o-mini"
    prompt: `Describe ${topic}. ${ctx.output_format()}`
}

// E0184 for a class with no JSON form, under a list.
function DescribeAll(topic: string) -> (Blob | string)[] {
    client: "openai/gpt-4o-mini"
    prompt: `Describe ${topic}. ${ctx.output_format()}`
}

// E0184 behind an alias.
function DescribeAliased(topic: string) -> BytesOrText {
    client: "openai/gpt-4o-mini"
    prompt: `Describe ${topic}. ${ctx.output_format()}`
}

// E0184 for an output with no JSON form at all: every call fails.
function DescribeBytes(topic: string) -> uint8array {
    client: "openai/gpt-4o-mini"
    prompt: `Describe ${topic}. ${ctx.output_format()}`
}

function DescribePhoto(topic: string) -> Photo {
    client: "openai/gpt-4o-mini"
    prompt: `Describe ${topic}. ${ctx.output_format()}`
}

// A generic return type is checked when it is called: no warning.
function DescribeAny<T>(topic: string) -> T | uint8array {
    client: "openai/gpt-4o-mini"
    prompt: `Describe ${topic}. ${ctx.output_format()}`
}

// A list of `never` is only the empty list, and `never[]` has a JSON form:
// no warning.
function DescribeNothing(topic: string) -> never[] {
    client: "openai/gpt-4o-mini"
    prompt: `Describe ${topic}. ${ctx.output_format()}`
}

// An ordinary function is called with bytes too: no warning.
function echo_bytes_or_text(value: uint8array | string) -> uint8array | string {
    value
}
