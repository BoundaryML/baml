class Box<T> {
    value: T,
}

function Identity<T>(value: T) -> T {
    Box<T> { value: value }.value
}

function WithDefault(value: int, offset: int = 10) -> int {
    value + offset
}

class Counter {
    value: int,

    function add(self, value: int) -> int {
        self.value += value;
        self.value
    }

    function options(self) -> trace.Options {
        self.value += 10;
        trace.options()
    }

    function echo<T>(self, value: T) -> T {
        Box<T> { value: value }.value
    }
}

interface Echo {
    function echo<T>(self, value: T) -> T throws never;
}

class EchoImpl {
    implements Echo {
        function echo<T>(self, value: T) -> T {
            Box<T> { value: value }.value
        }
    }
}

function ThroughInterface(value: Echo) -> string {
    value.echo<string>("interface", $trace = trace.options())
}

function OptionalCall(value: Counter?, span: trace.ReservedSpan) -> int? {
    value?.add(1, $trace = span)
}

test "optional calls do not consume unused reservations" {
    let span = trace.span().reserve();
    assert.equal(OptionalCall(null, span), null);
    assert.equal(Identity(42, $trace = span), 42)
}

test "trace call preserves direct generic specialization" {
    assert.equal(Identity<int>(42, $trace = trace.options()), 42);
    assert.equal(Identity("inferred", $trace = trace.options()), "inferred")
}

test "trace call specializes generic function values" {
    let identity = Identity<int>;
    assert.equal(identity(42, $trace = trace.span()), 42)
}

test "trace call preserves closure and bound method invocation" {
    let closure = (value: int) -> {
        value + 1
    };
    assert.equal(closure(41, $trace = trace.options()), 42);
    let counter = Counter { value: 0 };
    assert.equal(counter.echo<string>("method", $trace = trace.options()), "method");
    let bound = counter.add;
    assert.equal(bound(42, $trace = trace.options()), 42);
    assert.equal(ThroughInterface(EchoImpl {  }), "interface")
}

test "trace call preserves defaults and named argument layouts" {
    assert.equal(WithDefault(2, $trace = trace.options()), 12);
    assert.equal(WithDefault(offset = 3, $trace = trace.options(), value = 2), 5);
    assert.equal(WithDefault($trace = trace.options(), value = 2), 12)
}

test "trace operand evaluates in source order exactly once" {
    let counter = Counter { value: 0 };
    let result = WithDefault(value = counter.add(1), $trace = counter.options(), offset = counter.add(100));
    assert.equal(result, 112);
    assert.equal(counter.value, 111)
}

test "reserved span is accepted as the trace operand" {
    let reserved = trace.options().reserve();
    assert.equal(Identity(42, $trace = reserved), 42)
}

test "nested trace calls do not consume the outer override" {
    assert.equal(Identity(Identity(42, $trace = trace.hidden()), $trace = trace.options()), 42)
}

test "trace operand rejects invalid type and duplicate labels" {
    let invalid = {
        reflect.Package.compile(
            {
                "case.baml": `
                    function F() -> int { 1 }
                    function Main() -> int { F($trace = 42) }
                `,
            },
        );
        false
    } catch (error) {
        reflect.errors.CompilationError => true,
    };
    assert.equal(invalid, true);
    let duplicate = {
        reflect.Package.compile(
            {
                "case.baml": `
                    function F() -> int { 1 }
                    function Main() -> int {
                        F($trace = trace.options(), $trace = trace.options())
                    }
                `,
            },
        );
        false
    } catch (error) {
        reflect.errors.CompilationError => true,
    };
    assert.equal(duplicate, true)
}
