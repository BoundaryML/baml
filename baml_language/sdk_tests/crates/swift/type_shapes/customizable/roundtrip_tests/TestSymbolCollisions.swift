// Roundtrip coverage for the symbol-collision suite — port of
// python_pydantic2 `roundtrip_tests/test_symbol_collisions.py`. Three
// distinct `Bar` classes at different namespace depths plus the
// consumers (`Ipsum`, `Deep`) that compose all three.
import XCTest
import Baml

final class TestSymbolCollisions: XCTestCase {
    func test_symbol_collisions_round_trip_foo_bar() throws {
        let bar = try Baml.symbol_collisions.foo.make_foo_bar_sync(label: "hi", count: 2)
        XCTAssertEqual(try Baml.symbol_collisions.foo.round_trip_foo_bar_sync(b: bar), bar)
    }

    func test_symbol_collisions_round_trip_fizz_foo_bar() throws {
        let bar = try Baml.symbol_collisions.fizz.foo.make_fizz_foo_bar_sync(tag: "t", ratio: 1.5)
        XCTAssertEqual(try Baml.symbol_collisions.fizz.foo.round_trip_fizz_foo_bar_sync(b: bar), bar)
    }

    func test_symbol_collisions_round_trip_fizz_buzz_foo_bar() throws {
        let bar = try Baml.symbol_collisions.fizz.buzz.foo.make_fizz_buzz_foo_bar_sync(
            flavor: "f", weight: 2.5, active: true
        )
        XCTAssertEqual(
            try Baml.symbol_collisions.fizz.buzz.foo.round_trip_fizz_buzz_foo_bar_sync(b: bar),
            bar
        )
    }

    func test_symbol_collisions_round_trip_ipsum() throws {
        let ipsum = try Baml.symbol_collisions.lorem.make_ipsum_sync(
            bar1: Baml.symbol_collisions.foo.make_foo_bar_sync(label: "a", count: 1),
            bar2: Baml.symbol_collisions.fizz.foo.make_fizz_foo_bar_sync(tag: "b", ratio: 2.0),
            bar3: Baml.symbol_collisions.fizz.buzz.foo.make_fizz_buzz_foo_bar_sync(
                flavor: "c", weight: 3.0, active: false
            )
        )
        XCTAssertEqual(try Baml.symbol_collisions.lorem.round_trip_ipsum_sync(i: ipsum), ipsum)
    }

    func test_symbol_collisions_round_trip_deep() throws {
        let ipsum = try Baml.symbol_collisions.lorem.make_ipsum_sync(
            bar1: Baml.symbol_collisions.foo.make_foo_bar_sync(label: "a", count: 1),
            bar2: Baml.symbol_collisions.fizz.foo.make_fizz_foo_bar_sync(tag: "b", ratio: 2.0),
            bar3: Baml.symbol_collisions.fizz.buzz.foo.make_fizz_buzz_foo_bar_sync(
                flavor: "c", weight: 3.0, active: false
            )
        )
        let deep = try Baml.symbol_collisions.a.b.c.d.make_deep_sync(
            here: Baml.symbol_collisions.foo.make_foo_bar_sync(label: "h", count: 9),
            there: Baml.symbol_collisions.fizz.foo.make_fizz_foo_bar_sync(tag: "th", ratio: 4.0),
            further: Baml.symbol_collisions.fizz.buzz.foo.make_fizz_buzz_foo_bar_sync(
                flavor: "fu", weight: 5.0, active: true
            ),
            nested: ipsum
        )
        XCTAssertEqual(try Baml.symbol_collisions.a.b.c.d.round_trip_deep_sync(d: deep), deep)
    }
}
