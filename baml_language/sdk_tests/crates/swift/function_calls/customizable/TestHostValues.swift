// Host values: a key of this process's own registry crosses into BAML, which
// holds it by reference and sends it back. Swift has no public host-value
// type, so these tests register one directly.
import XCTest
import Baml
@testable import BamlBridge

private struct Held: Error {
    let name: String
}

private var registry: HostCallableRegistry { .shared }

/// A host value registered for these tests, holding the one reference its
/// registration takes; the test gives it back.
private func registerHostValue(_ name: String) -> BamlHandle {
    BamlHandle(key: registry.register(.opaqueError(Held(name: name))), handleType: .hostValueOpaque)
}

/// Collects until the engine has given back every reference to `key` but
/// `held`; false if it still holds one after a few seconds.
private func releasedByTheEngine(_ key: UInt64, held: UInt64) throws -> Bool {
    let deadline = Date().addingTimeInterval(5)
    repeat {
        try Baml.baml.sys.collect_garbage()
        if registry.references(to: key) == held { return true }
    } while Date() < deadline
    return false
}

final class TestHostValues: XCTestCase {
    func test_host_values_round_trip_as_the_same_value() throws {
        let sent = registerHostValue("mine")
        defer { registry.release(sent.key, count: 1) }
        let back = try Baml.host_value_tests.echo_host_value(value: sent)
        XCTAssertEqual(back, sent)
    }

    func test_host_values_sent_twice_are_one_value_to_baml() throws {
        let sent = registerHostValue("twice")
        let other = registerHostValue("other")
        defer {
            registry.release(sent.key, count: 1)
            registry.release(other.key, count: 1)
        }
        XCTAssertTrue(try Baml.host_value_tests.same_host_value(a: sent, b: sent))
        // Negative control: two host values are two values.
        XCTAssertFalse(try Baml.host_value_tests.same_host_value(a: sent, b: other))
    }

    func test_host_value_decoded_twice_is_one_value() throws {
        let sent = registerHostValue("decoded twice")
        defer { registry.release(sent.key, count: 1) }
        let back = try Baml.host_value_tests.twice_host_value(value: sent)
        XCTAssertEqual(back.count, 2)
        XCTAssertEqual(back[0], back[1])
        XCTAssertEqual(back[0], sent)
    }

    // A host value BAML sends back decodes even when a collection ran between
    // the result and the decode: the result's pin holds the engine's
    // references. The pin goes back with the result, so once BAML lets the
    // value go, only the test's own reference is left.
    func test_host_value_back_decodes_after_baml_collects_garbage() throws {
        let sent = registerHostValue("pinned")
        let key = sent.key
        for _ in 0..<16 {
            let back = try Baml.host_value_tests.echo_host_value(value: sent)
            try Baml.baml.sys.collect_garbage()
            XCTAssertEqual(back, sent)
            // Sent again under the key it came back with.
            XCTAssertTrue(try Baml.host_value_tests.same_host_value(a: back, b: sent))
        }
        XCTAssertTrue(try releasedByTheEngine(key, held: 1))
        registry.release(key, count: 1)
        XCTAssertNil(registry.lookup(key))
        // A key this process no longer resolves is never sent.
        XCTAssertThrowsError(try Baml.host_value_tests.echo_host_value(value: sent))
    }

    // A call that never starts gives back exactly the references its
    // arguments took: the value held elsewhere keeps the rest.
    func test_host_values_of_a_rolled_back_call_give_back_only_its_references() throws {
        let held = registerHostValue("held")
        let gone = registerHostValue("gone")
        registry.release(gone.key, count: 1)
        XCTAssertThrowsError(try Baml.host_value_tests.same_host_value(a: held, b: gone))
        XCTAssertEqual(registry.references(to: held.key), 1)
        registry.release(held.key, count: 1)
        XCTAssertNil(registry.lookup(held.key))
    }

    // A Swift error a callback raised crosses as a host key inside the
    // `baml.errors.HostCallable` BAML builds around it, and comes back as the
    // original error. Once BAML lets the error go, its key is given back:
    // the error envelope's pin went back with it.
    func test_host_values_raised_by_a_callback_are_given_back() throws {
        let before = registry.registeredKeys
        for attempt in 0..<16 {
            let raised = Held(name: "raised \(attempt)")
            do {
                _ = try Baml.host_callable_tests.propagate_throwing_callback(
                    callback: { _ in throw raised },
                    value: attempt
                )
                XCTFail("expected the raised error")
            } catch let error as Held {
                XCTAssertEqual(error.name, raised.name)
            }
            try Baml.baml.sys.collect_garbage()
        }
        let deadline = Date().addingTimeInterval(5)
        while !registry.registeredKeys.isSubset(of: before), Date() < deadline {
            try Baml.baml.sys.collect_garbage()
        }
        XCTAssertTrue(registry.registeredKeys.isSubset(of: before))
    }
}
