// Schema-aligned parsing into generic classes. A generic class is a type
// constructor: `Box<int>` and `Box<string>` are different types with
// different field schemas, so the parser models each instantiation a target
// reaches, and a parsed instance carries the type arguments of its type.

class Box<T> {
    item: T,
}

class BoxPair {
    ints: Box<int>,
    strings: Box<string>,
}

class Tree<T> {
    value: T,
    children: Tree<T>[],
}

class WithSkipped<T> {
    item: T,
    history: T[] @skip,
}

class Nest<T> {
    inner: Nest<T[]>?,
}

enum Shade {
    Light,
    Dark,
}

class Note {
    title: string,
}

function parse_box<T>(raw: string) -> Box<T> {
    baml.sap.parse<Box<T>>(raw)
}

test "a generic class parses at its type argument" {
    assert.equal(baml.sap.parse<Box<int>>(`{"item": 3}`).item, 3)
}

test "a generic class parses through a caller's type parameter" {
    assert.equal(parse_box<string>(`{"item": "boxed"}`).item, "boxed")
}

test "two instantiations of one class align their own fields" {
    let pair = baml.sap.parse<BoxPair>(`{"ints": {"item": 1}, "strings": {"item": "one"}}`);
    assert.equal(pair.ints.item, 1);
    assert.equal(pair.strings.item, "one")
}

test "a list of instantiations parses each element" {
    let boxes = baml.sap.parse<Box<int>[]>(`[{"item": 1}, {"item": 2}]`);
    assert.equal(boxes.length(), 2);
    assert.equal(boxes.at(1)?.item, 2)
}

test "a union of instantiations picks the one the value aligns to" {
    let parsed = baml.sap.parse<Box<int> | Box<string>>(`{"item": "text"}`);
    let picked = match (parsed) {
        let ints: Box<int> => `int ${ints.item}`,
        let strings: Box<string> => `string ${strings.item}`,
    };
    assert.equal(picked, "string text")
}

test "a recursive generic class parses at one instantiation" {
    let tree = baml.sap.parse<Tree<int>>(
        `{"value": 1, "children": [{"value": 2, "children": []}]}`,
    );
    assert.equal(tree.children.at(0)?.value, 2)
}

test "an enum type argument aligns to its variants" {
    assert.is_true(baml.sap.parse<Box<Shade>>(`{"item": "Dark"}`).item == Shade.Dark)
}

test "a parsed instance carries its type arguments" {
    let parsed: unknown = baml.sap.parse<Box<int>>(`{"item": 3}`);
    let kind = match (parsed) {
        let strings: Box<string> => `Box<string> ${strings.item}`,
        let ints: Box<int> => `Box<int> ${ints.item}`,
        _ => "neither",
    };
    assert.equal(kind, "Box<int> 3")
}

test "a skipped field holds the empty value of its instantiated type" {
    let parsed = baml.sap.parse<WithSkipped<int>>(`{"item": 4}`);
    assert.equal(parsed.item, 4);
    assert.equal(parsed.history.length(), 0)
}

test "coerce aligns a parsed value to a generic class" {
    let json: json = {"item": 5};
    assert.equal(baml.sap.coerce<Box<int>>(json).item, 5)
}

function coerce_non_regular() -> string {
    let json: json = {"inner": null};
    let _ = baml.sap.coerce<Nest<int>>(json) catch (e) {
        baml.errors.ParseError { message } => {
            return message
        },
    };
    "coerced"
}

test "a class whose instantiations never close cannot be modeled" {
    assert.equal(
        coerce_non_regular(),
        "schema-aligned parsing cannot model this type: non-regular recursive generic class `sap_generic_classes.Nest` expands from `sap_generic_classes.Nest<int>` to `sap_generic_classes.Nest<int[]>`",
    )
}

// An LLM function returning a generic class, driven by an offline client.

function ExtractBoxedNote(input: string) -> Box<Note> {
    client: "openai/gpt-4o-mini"
    prompt: `Summarize ${input}. ${ctx.output_format()}`
}

class CannedClient {
    chunks: string[],

    implements ai.Client {
        function id(self) -> string {
            "canned"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request {
                method: "POST",
                url: "https://example.invalid/canned",
                headers: {},
                body: "{}",
            }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            ai.ModelTurn {
                calls: [],
                content: [ai.content.Text { text: self.chunks.join("") }],
                stop_reason: ai.content.StopReason.Complete,
                usage: null,
            }
        }
    }

    implements ai.stream.StreamingClient {
        function invoke_stream(self, input: ai.ModelTurnInput) -> ai.stream.TurnStream {
            let _ = input;
            ai.stream.TurnStream.from_chunks(self.chunks)
        }
    }
}

test "an LLM function returning a generic class parses its reply" {
    let canned = CannedClient { chunks: [`{"item": {"title": "Hi"}}`] };
    let boxed = ai.Agent.new(client = canned).run(ExtractBoxedNote@spec("x")).value;
    assert.equal(boxed.item.title, "Hi")
}

function stream_boxed_note(chunks: string[]) -> string {
    let stream = ai.stream.from_spec<Box<Note>>(
        ExtractBoxedNote@spec("x"),
        client = CannedClient { chunks: chunks },
    );
    let partials = 0;
    while let partial: Box<Note> = stream.next() {
        let _ = partial;
        partials += 1;
    }
    `${partials > 0}|${stream.final().item.title}`
}

test "a streamed generic class yields partials and a final value" {
    assert.equal(
        stream_boxed_note([`{"item": `, `{"title": "H`, `i"}}`]),
        "true|Hi",
    )
}

// A type argument the parsed fields never reach still lands on the instance:
// a phantom parameter, one only a `@skip` field uses, an interface argument,
// and a generic field of a class that is not itself generic.

class Tagged<T> {
    name: string,
}

class SkipOnly<T> {
    name: string,
    cache: T? @skip,
}

class TaggedNotes {
    tags: Tagged<Note>[],
}

interface Speaker {
    function speak(self) -> string throws never
}

test "a phantom type argument lands on the instance" {
    let parsed: unknown = baml.sap.parse<Tagged<Shade>>(`{"name": "a"}`);
    let kind = match (parsed) {
        let tagged: Tagged<Shade> => `Tagged<Shade> ${tagged.name}`,
        _ => "other",
    };
    assert.equal(kind, "Tagged<Shade> a")
}

test "a type argument only a skipped field uses lands on the instance" {
    let parsed = baml.sap.parse<SkipOnly<Note>>(`{"name": "b"}`);
    assert.equal(parsed.name, "b");
    assert.equal(parsed.cache, null)
}

test "an interface type argument lands on the instance" {
    let parsed: unknown = baml.sap.parse<Tagged<Speaker>>(`{"name": "c"}`);
    let kind = match (parsed) {
        let tagged: Tagged<Speaker> => `Tagged<Speaker> ${tagged.name}`,
        _ => "other",
    };
    assert.equal(kind, "Tagged<Speaker> c")
}

test "a class that is not generic parses its generic fields" {
    let parsed = baml.sap.parse<TaggedNotes>(`{"tags": [{"name": "d"}]}`);
    assert.equal(parsed.tags.at(0)?.name, "d")
}

// An interface holds a value of some implementor, and a parse has none to
// build: nothing parses as one.

test "an interface member of a union never matches" {
    let parsed = baml.sap.parse<Speaker | int>(`3`);
    let kind = match (parsed) {
        let n: int => `int ${n}`,
        _ => "other",
    };
    assert.equal(kind, "int 3")
}

function parse_interface() -> string {
    let _ = baml.sap.parse<Speaker>(`{"name": "e"}`) catch (e) {
        baml.errors.ParseError { message } => {
            return "ParseError"
        },
    };
    "parsed"
}

test "an interface target throws a parse error" {
    assert.equal(parse_interface(), "ParseError")
}

// A class whose argument grows only through unions has finitely many
// instantiations: `(int?)?` is `int?`, and `(string | int) | int` is
// `string | int`.

class Opt<T> {
    value: T,
    inner: Opt<T?>?,
}

class Grow<T> {
    value: T,
    next: Grow<T | int>?,
}

class Swap<A, B> {
    first: A,
    next: Swap<B, A | B>?,
}

class Ping<T> {
    pong: Pong<T[]>?,
}

class Pong<T> {
    ping: Ping<T>?,
}

test "a recursion through optionals parses at its instantiations" {
    let parsed = baml.sap.parse<Opt<string>>(
        `{"value": "a", "inner": {"value": null, "inner": {"value": "c", "inner": null}}}`,
    );
    let inner: unknown = parsed.inner?.inner;
    let kind = match (inner) {
        let opt: Opt<string?> => `Opt<string?> ${opt.value ?? "null"}`,
        _ => "other",
    };
    assert.equal(kind, "Opt<string?> c")
}

test "a recursion through a union with a constant parses at its instantiations" {
    let parsed = baml.sap.parse<Grow<string>>(
        `{"value": "s", "next": {"value": 1, "next": {"value": "t", "next": null}}}`,
    );
    let next: unknown = parsed.next?.next;
    let kind = match (next) {
        let grow: Grow<string | int> => "Grow<string | int>",
        _ => "other",
    };
    assert.equal(kind, "Grow<string | int>")
}

test "a recursion that permutes its union arguments parses" {
    let parsed = baml.sap.parse<Swap<int, string>>(
        `{"first": 1, "next": {"first": "x", "next": {"first": 2, "next": null}}}`,
    );
    let next: unknown = parsed.next;
    let kind = match (next) {
        let swap: Swap<string, int | string> => "Swap<string, int | string>",
        _ => "other",
    };
    assert.equal(kind, "Swap<string, int | string>")
}

function coerce_mutual_non_regular() -> string {
    let json: json = {"pong": null};
    let _ = baml.sap.coerce<Ping<int>>(json) catch (e) {
        baml.errors.ParseError { message } => {
            return message
        },
    };
    "coerced"
}

test "a mutual recursion whose arguments grow cannot be modeled" {
    assert.equal(
        coerce_mutual_non_regular(),
        "schema-aligned parsing cannot model this type: non-regular recursive generic class `sap_generic_classes.Ping` expands from `sap_generic_classes.Ping<int>` to `sap_generic_classes.Ping<int[]>`",
    )
}

function ExtractOptNote(input: string) -> Opt<string> {
    client: "openai/gpt-4o-mini"
    prompt: `Summarize ${input}. ${ctx.output_format()}`
}

test "an LLM function returning a recursion through optionals renders and parses" {
    let canned = CannedClient { chunks: [`{"value": "a", "inner": {"value": null, "inner": null}}`] };
    let parsed = ai.Agent.new(client = canned).run(ExtractOptNote@spec("x")).value;
    assert.equal(parsed.value, "a");
    assert.equal(parsed.inner?.value, null)
}

// A generic class reflected without its arguments names no type.

function coerce_unspecialized_generic() -> string {
    let package = reflect.Package.compile({ "main.baml": `
        class RBox<T> {
            item: T,
        }
    ` });
    let class_type = package.get_class("RBox");
    let type T = class_type.as_type();
    let json: json = {"item": 1};
    let _ = baml.sap.coerce<T>(json) catch (e) {
        baml.errors.ParseError { message } => {
            return message
        },
    };
    "coerced"
}

test "a generic class without its type arguments cannot be modeled" {
    assert.equal(
        coerce_unspecialized_generic(),
        "schema-aligned parsing cannot model this type: class `RBox` takes 1 type argument, but is given 0",
    )
}
