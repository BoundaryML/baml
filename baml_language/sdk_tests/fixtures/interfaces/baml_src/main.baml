// Shared bridge contract. Native tests call these same entrypoints in every SDK.
// Keep invalid BAML in the compiler diagnostic corpus, not in this fixture.

interface Greeter {
    function greet(self, name: string) -> string throws never
    function label(self) -> string throws never { "greeter" }
}

class FriendlyGreeter {
    prefix: string,

    function new(prefix: string) -> FriendlyGreeter throws never {
        FriendlyGreeter { prefix: prefix }
    }

    implements Greeter {
        function greet(self, name: string) -> string throws never {
            `${self.prefix}, ${name}!`
        }
    }
}

function make_greeter(prefix: string) -> Greeter throws never {
    FriendlyGreeter.new(prefix)
}

function pass_greeter(value: Greeter) -> Greeter throws never {
    value
}

function welcome(value: Greeter, name: string) -> string throws never {
    value.greet(name)
}

function greeter_label(value: Greeter) -> string throws never {
    value.label()
}

interface Counter {
    function add(self, amount: int) -> int throws never
    function current(self) -> int throws never
}

interface ExtendedCounter requires Counter {}

interface CounterValue {
    type Value
    type Error = never
    function update(self, amount: Self.Value) -> Self.Value throws Self.Error
}

interface UnspecifiedCounter requires CounterValue {}

class IntCounter {
    count: int,
    implements CounterValue {
        type Value = int
        function update(self, amount: int) -> int throws never {
            self.count += amount;
            self.count
        }
    }
    implements UnspecifiedCounter {}
}

function make_unspecified_counter(initial: int) -> UnspecifiedCounter throws never {
    IntCounter { count: initial }
}

class StoredCounter {
    count: int,
    implements ExtendedCounter {}

    implements Counter {
        function add(self, amount: int) -> int throws never {
            self.count += amount;
            self.count
        }
        function current(self) -> int throws never {
            self.count
        }
    }
}

class CounterRecord {
    title: string,
    counter: Counter,
}

function make_counter(initial: int) -> Counter throws never {
    StoredCounter { count: initial }
}

function report_counter(initial: int) -> int {
    let counter = make_counter(initial);
    spawn { throw counter; };
    1
}

function visit_counter(initial: int, callback: (Counter) -> int) -> int {
    callback(make_counter(initial))
}

function make_extended_counter(initial: int) -> ExtendedCounter throws never {
    StoredCounter { count: initial }
}

function pass_counter(value: Counter) -> Counter throws never {
    value
}

function add_in_baml(value: Counter, amount: int) -> int throws never {
    value.add(amount)
}

function counter_record(value: Counter) -> CounterRecord throws never {
    CounterRecord { title: "counter", counter: value }
}

interface Decoder {
    type Output
    type Error = never
    function decode(self, input: image) -> Self.Output throws Self.Error
}

class TextDecoder {
    implements Decoder {
        type Output = string
        type Error = never
        function decode(self, input: image) -> string throws never {
            input.base64()
        }
    }
}

function make_text_decoder() -> Decoder<Output = string, Error = never> throws never {
    TextDecoder {  }
}

function decode_text(
    value: Decoder<Output = string, Error = never>,
    input: image,
) -> string throws never {
    value.decode(input)
}

interface Echo {
    function echo<T>(self, value: T) -> T throws never
}

class IdentityEcho {
    implements Echo {
        function echo<T>(self, value: T) -> T throws never {
            value
        }
    }
}

function make_echo() -> Echo throws never {
    IdentityEcho {  }
}

function echo_text(value: Echo, text: string) -> string throws never {
    value.echo(text)
}

function echo_counter(value: Echo, counter: Counter) -> Counter throws never {
    value.echo(counter)
}

function as_string_iterable(
    values: string[],
) -> baml.iter.Iterable<Item = string, Error = never> throws never {
    values
}

function as_character_iterable(
    value: string,
) -> baml.iter.Iterable<Item = string, Error = never> throws never {
    value
}

function pass_iterable(
    value: baml.iter.Iterable<Item = string, Error = never>,
) -> baml.iter.Iterable<Item = string, Error = never> throws never {
    value
}

function next_in_baml(
    value: baml.iter.Iterator<Item = string, Error = never>,
) -> string | baml.iter.Done throws never {
    value.next()
}

function nullable_iterator(
    values: string?[],
) -> baml.iter.Iterator<Item = string?, Error = never> throws never {
    values.iter()
}

function image_callback(
    value: image,
    callback: (image) -> image throws never,
) -> image throws never {
    callback(value)
}

function pass_callback(value: (string) -> string throws never) -> (string) -> string throws never {
    value
}

// A marker implementation must not change a record's copied-data codec.
interface RecordMarker {}

class MarkedRecord {
    text: string,
    implements RecordMarker {}
}

function marker_record(text: string) -> MarkedRecord throws never {
    MarkedRecord { text }
}

function pass_marker(value: RecordMarker) -> RecordMarker throws never {
    value
}

function marker_text(value: RecordMarker) -> string throws never {
    match (value) {
        let record: MarkedRecord => record.text,
        _ => "other",
    }
}

function make_counter_factory(initial: int) -> () -> Counter throws never {
    let counter = make_counter(initial);
    () -> {
        counter
    }
}

function call_counter_factory(factory: () -> Counter throws never) -> Counter throws never {
    factory()
}

type CounterVisitor = (Counter) -> int throws never;

type OptionalCounterVisitor = (value: Counter, amount?: int) -> int throws never;

type CounterList = Counter[];

type CounterListAlias = CounterList;

function visit_counter_alias(initial: int, callback: CounterVisitor) -> int {
    callback(make_counter(initial))
}

function visit_counter_optional(initial: int, callback: OptionalCounterVisitor) -> int {
    callback(make_counter(initial)) + callback(make_counter(initial), amount = 2)
}

function add_counter_list(values: CounterListAlias, amount: int) -> int[] {
    values.map((value) -> {
        value.add(amount)
    })
}

function add_counter_map(values: map<string, Counter>, amount: int) -> int[] {
    values.values().map((value) -> {
        value.add(amount)
    })
}

function make_stored_counter(initial: int) -> StoredCounter throws never {
    StoredCounter { count: initial }
}

function use_counter_factory_callback(
    callback: (() -> Counter throws never) -> int throws never,
) -> int {
    callback(make_counter_factory(10))
}

function visit_generic_callback<T>(
    value: T,
    callback: (value: T, other?: T) -> T throws never,
) -> T throws never {
    callback(value, other = value)
}

interface CounterCallbackRunner {
    function visit(self, callback: OptionalCounterVisitor) -> int throws never
    function choose<T>(self, value: T, callback: (value: T, other?: T) -> T throws never) -> T throws never
}

class StoredCounterRunner {
    counter: Counter,
    implements CounterCallbackRunner {
        function visit(self, callback: OptionalCounterVisitor) -> int throws never {
            callback(self.counter, amount = 2)
        }
        function choose<T>(
            self,
            value: T,
            callback: (value: T, other?: T) -> T throws never,
        ) -> T throws never {
            callback(value, other = value)
        }
    }
}

function make_counter_runner(initial: int) -> CounterCallbackRunner throws never {
    StoredCounterRunner { counter: make_counter(initial) }
}

// Nominal enum evidence must survive a method's per-call generic frame.
enum InterfaceFlavor {
    Vanilla,
    Chocolate,
}

interface BoundedMethod {
    function check<T extends RecordMarker, U>(self) -> string throws never
}

class BoundedMethodImpl {
    implements BoundedMethod {
        function check<T extends RecordMarker, U>(self) -> string throws never {
            "checked"
        }
    }
}

function make_bounded_method() -> BoundedMethod throws never {
    BoundedMethodImpl {  }
}
