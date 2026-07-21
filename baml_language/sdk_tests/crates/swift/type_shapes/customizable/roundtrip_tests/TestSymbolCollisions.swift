// Roundtrip coverage for the symbol-collision suite — port of
// python_pydantic2 `roundtrip_tests/test_symbol_collisions.py`. Three
// distinct `Bar` classes at different namespace depths plus the
// consumers (`Ipsum`, `Deep`) that compose all three.
import XCTest
import Baml

final class TestSymbolCollisions: XCTestCase {
    func test_symbol_collisions_round_trip_foo_bar() throws {
        let bar = try Baml.symbol_collisions.foo.make_foo_bar(label: "hi", count: 2)
        XCTAssertEqual(try Baml.symbol_collisions.foo.round_trip_foo_bar(b: bar), bar)
    }

    func test_symbol_collisions_round_trip_fizz_foo_bar() throws {
        let bar = try Baml.symbol_collisions.fizz.foo.make_fizz_foo_bar(tag: "t", ratio: 1.5)
        XCTAssertEqual(try Baml.symbol_collisions.fizz.foo.round_trip_fizz_foo_bar(b: bar), bar)
    }

    func test_symbol_collisions_round_trip_fizz_buzz_foo_bar() throws {
        let bar = try Baml.symbol_collisions.fizz.buzz.foo.make_fizz_buzz_foo_bar(
            flavor: "f", weight: 2.5, active: true
        )
        XCTAssertEqual(
            try Baml.symbol_collisions.fizz.buzz.foo.round_trip_fizz_buzz_foo_bar(b: bar),
            bar
        )
    }

    func test_symbol_collisions_round_trip_ipsum() throws {
        let ipsum = try Baml.symbol_collisions.lorem.make_ipsum(
            bar1: Baml.symbol_collisions.foo.make_foo_bar(label: "a", count: 1),
            bar2: Baml.symbol_collisions.fizz.foo.make_fizz_foo_bar(tag: "b", ratio: 2.0),
            bar3: Baml.symbol_collisions.fizz.buzz.foo.make_fizz_buzz_foo_bar(
                flavor: "c", weight: 3.0, active: false
            )
        )
        XCTAssertEqual(try Baml.symbol_collisions.lorem.round_trip_ipsum(i: ipsum), ipsum)
    }

    func test_symbol_collisions_round_trip_deep() throws {
        let ipsum = try Baml.symbol_collisions.lorem.make_ipsum(
            bar1: Baml.symbol_collisions.foo.make_foo_bar(label: "a", count: 1),
            bar2: Baml.symbol_collisions.fizz.foo.make_fizz_foo_bar(tag: "b", ratio: 2.0),
            bar3: Baml.symbol_collisions.fizz.buzz.foo.make_fizz_buzz_foo_bar(
                flavor: "c", weight: 3.0, active: false
            )
        )
        let deep = try Baml.symbol_collisions.a.b.c.d.make_deep(
            here: Baml.symbol_collisions.foo.make_foo_bar(label: "h", count: 9),
            there: Baml.symbol_collisions.fizz.foo.make_fizz_foo_bar(tag: "th", ratio: 4.0),
            further: Baml.symbol_collisions.fizz.buzz.foo.make_fizz_buzz_foo_bar(
                flavor: "fu", weight: 5.0, active: true
            ),
            nested: ipsum
        )
        XCTAssertEqual(try Baml.symbol_collisions.a.b.c.d.round_trip_deep(d: deep), deep)
    }
}
