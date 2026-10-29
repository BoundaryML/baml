// Instances the runtime keeps to itself (`baml.hash.DefaultHasher`) cross as
// references: one passed back is the same instance, wherever a class can sit.
// Its state shows which instance a call reached: one hasher fed "a" and then
// "b" hashes as a fresh one fed "ab".
import Foundation
import XCTest
import Baml
import BamlBridge

private typealias R = Baml.references
private typealias DefaultHasher = Baml.baml.hash.DefaultHasher

/// The values a callback received, across the threads it runs on.
private final class Received: @unchecked Sendable {
    private let lock = NSLock()
    private var values: [DefaultHasher] = []

    func append(_ value: DefaultHasher) {
        lock.withLock { values.append(value) }
    }

    var all: [DefaultHasher] { lock.withLock { values } }
}

final class TestReferences: XCTestCase {
    func test_references_a_returned_instance_passed_back_is_the_same_instance() throws {
        let hasher = try R.make_hasher()
        _ = try R.hash_after(hasher: hasher, text: "a")
        let fedTwice = try R.hash_after(hasher: hasher, text: "b")
        XCTAssertEqual(fedTwice, try R.hash_after(hasher: try R.make_hasher(), text: "ab"))
        // Negative control: a fresh hasher fed only "b" differs.
        XCTAssertNotEqual(fedTwice, try R.hash_after(hasher: try R.make_hasher(), text: "b"))
    }

    func test_references_one_engine_instance_is_one_reference() throws {
        let hasher = try R.make_hasher()
        XCTAssertEqual(try R.same_hasher(hasher: hasher), hasher)
        XCTAssertNotEqual(try R.make_hasher(), hasher)
    }

    func test_references_containers_hold_the_instance() throws {
        let hasher = try R.make_hasher()
        XCTAssertNotNil(try R.maybe_hasher(present: true))
        XCTAssertNil(try R.maybe_hasher(present: false))
        XCTAssertEqual(try R.hasher_twice(hasher: hasher), [hasher, hasher])
        XCTAssertEqual(try R.hasher_under(key: "key", hasher: hasher), ["key": hasher])
    }

    func test_references_a_field_holds_the_instance() throws {
        let hasher = try R.make_hasher()
        XCTAssertEqual(
            try R.hold_hasher(label: "label", hasher: hasher),
            R.HasherHolder(label: "label", hasher: hasher)
        )
    }

    func test_references_a_union_selects_the_instance_arm() throws {
        guard case .t0 = try R.hasher_or_marker(want_hasher: true) else {
            return XCTFail("hasher_or_marker(true) is not the hasher arm")
        }
        guard case .t1(let marker) = try R.hasher_or_marker(want_hasher: false) else {
            return XCTFail("hasher_or_marker(false) is not the marker arm")
        }
        XCTAssertEqual(marker, R.HasherMarker(tag: "marker"))
        XCTAssertEqual(try R.describe_hasher_or_marker(value: .t0(try R.make_hasher())), "hasher")
        XCTAssertEqual(
            try R.describe_hasher_or_marker(value: .t1(R.HasherMarker(tag: "marker"))),
            "marker"
        )
    }

    func test_references_a_callback_receives_and_returns_the_instance() throws {
        let hasher = try R.make_hasher()
        let received = Received()
        let returned = try R.hasher_through_callback(hasher: hasher, callback: { value in
            received.append(value)
            return value
        })
        XCTAssertEqual(returned, hasher)
        XCTAssertEqual(received.all, [hasher])
    }
}
