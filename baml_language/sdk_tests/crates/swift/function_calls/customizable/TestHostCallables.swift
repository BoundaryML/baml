// Host callables — port of python_pydantic2 `test_host_callables.py`.
// BAML invoking Swift closures, including throws crossing both ways.
//
// Adaptations from Python noted inline:
// - Python asserts thrown-exception *identity* (`exc.value is raised`);
//   Swift errors are values, so identity becomes structural equality
//   on a unique payload.
// - Python's `BamlError(ValidationError(...))` raised inside a
//   callback is Swift's `BamlThrownValue(ValidationError(...))`.
// - test_release_fires_on_drop_of_callable is xfail even in Python
//   (GC-timing dependent); not ported.
// - Async callables are native here (the generated closure type is
//   `async throws`), so the async cases are plain ports.
import XCTest
import Baml
import BamlBridge

private struct TestError: Error, Equatable {
    let tag: String
    var code: Int = 0
}

/// Shared mutable state for callbacks under Swift 6 strict concurrency.
private final class LockedBox<T>: @unchecked Sendable {
    private let lock = NSLock()
    private var value: T
    init(_ value: T) { self.value = value }
    func with<R>(_ body: (inout T) -> R) -> R {
        lock.lock()
        defer { lock.unlock() }
        return body(&value)
    }
    var snapshot: T {
        lock.lock()
        defer { lock.unlock() }
        return value
    }
}

private func orDefault(_ slot: BamlOptional<Int>, _ defaultValue: Int) -> Int {
    switch slot {
    case .unset: return defaultValue
    case .null: return 0
    case .value(let v): return v
    }
}

final class TestHostCallables: XCTestCase {
    func test_host_callables_simple_sync_callable_returns_string() throws {
        let result = try Baml.host_callable_tests.call_with_callback(
            callback: { x in "got \(x)" },
            x: 5
        )
        XCTAssertEqual(result, "got 5")
    }

    func test_host_callables_two_arg_callable_unpacks_positional_args() throws {
        let result = try Baml.host_callable_tests.call_with_two_args(
            callback: { x, prefix in "\(prefix):\(x)" },
            x: 7,
            prefix: "answer"
        )
        XCTAssertEqual(result, "answer:7")
    }

    func test_host_callables_int_return_callable_round_trip() throws {
        let result = try Baml.host_callable_tests.call_int_callback(
            callback: { x in x * 2 },
            x: 21
        )
        XCTAssertEqual(result, 42)
    }

    func test_baml_closure_is_a_native_callable_with_host_language_arguments() async throws {
        let addTen = try Baml.host_callable_tests.make_adder(offset: 10)
        let first = try await addTen(5)
        let second = try await addTen(7)
        XCTAssertEqual(first, 15)
        XCTAssertEqual(second, 17)
    }

    func test_baml_closure_decodes_multiple_args_and_structured_return_values() async throws {
        let build = try Baml.host_callable_tests.make_pair_builder(base: 30)
        let ada = try await build(12, "Ada")
        let grace = try await build(5, "Grace")
        XCTAssertEqual(ada, Baml.host_callable_tests.Person(name: "Ada", age: 42))
        XCTAssertEqual(grace, Baml.host_callable_tests.Person(name: "Grace", age: 35))
    }

    func test_baml_closure_is_reusable_and_retains_mutable_captures() async throws {
        let nextValue = try Baml.host_callable_tests.make_counter(start: 40)
        let first = try await nextValue()
        let second = try await nextValue()
        XCTAssertEqual(first, 41)
        XCTAssertEqual(second, 42)
    }

    func test_host_callables_throwing_callable_round_trips_original_host_exception() throws {
        let raised = TestError(tag: "nope")
        do {
            _ = try Baml.host_callable_tests.call_with_callback(
                callback: { _ in throw raised },
                x: 1
            )
            XCTFail("expected TestError")
        } catch let error as TestError {
            // Same-process rehydration: the ORIGINAL Swift error value.
            XCTAssertEqual(error, raised)
        }
    }

    func test_host_callables_throwing_callable_custom_host_exception_round_trips_with_identity() throws {
        let raised = TestError(tag: "custom domain failure", code: 42)
        do {
            _ = try Baml.host_callable_tests.call_with_callback(
                callback: { _ in throw raised },
                x: 1
            )
            XCTFail("expected TestError")
        } catch let error as TestError {
            XCTAssertEqual(error.code, 42)
        }
    }

    func test_host_callables_throwing_callable_hostthrow_codegenned_class_is_caught_in_baml() throws {
        let result = try Baml.host_callable_tests.call_with_typed_throws(
            callback: { _ in
                throw BamlThrownValue(
                    Baml.host_callable_tests.ValidationError(
                        code: 4,
                        message: "bad shape",
                        fields: ["name", "age", "email", "phone"]
                    )
                )
            },
            x: 1
        )
        XCTAssertEqual(result, "caught: bad shape")
    }

    func test_host_callables_throwing_callable_hostthrow_propagates_back_with_typed_fields() throws {
        do {
            _ = try Baml.host_callable_tests.call_with_typed_throws_propagating(
                callback: { _ in
                    throw BamlThrownValue(
                        Baml.host_callable_tests.ValidationError(
                            code: 7,
                            message: "propagated through",
                            fields: ["x", "y"]
                        )
                    )
                },
                x: 1
            )
            XCTFail("expected BamlError")
        } catch let error as BamlError {
            let decoded = try error.value(as: Baml.host_callable_tests.ValidationError.self)
            XCTAssertEqual(decoded.code, 7)
            XCTAssertEqual(decoded.message, "propagated through")
            XCTAssertEqual(decoded.fields, ["x", "y"])
        }
    }

    func test_host_callables_throwing_async_callable_round_trips_original_error() throws {
        let raised = TestError(tag: "async nope")
        do {
            _ = try Baml.host_callable_tests.call_with_callback(
                callback: { _ in
                    await Task.yield()
                    throw raised
                },
                x: 1
            )
            XCTFail("expected TestError")
        } catch let error as TestError {
            XCTAssertEqual(error, raised)
        }
    }

    func test_host_callables_multiple_throws_in_flight_do_not_collide_in_registry() throws {
        let first = TestError(tag: "first")
        let second = TestError(tag: "second")
        var caughtFirst: TestError?
        var caughtSecond: TestError?
        do {
            _ = try Baml.host_callable_tests.call_with_callback(callback: { _ in throw first }, x: 1)
        } catch let error as TestError { caughtFirst = error } catch {}
        do {
            _ = try Baml.host_callable_tests.call_with_callback(callback: { _ in throw second }, x: 2)
        } catch let error as TestError { caughtSecond = error } catch {}
        XCTAssertEqual(caughtFirst, first)
        XCTAssertEqual(caughtSecond, second)
        XCTAssertNotEqual(caughtFirst, caughtSecond)
    }

    func test_host_callables_lambda_round_trip() throws {
        let result = try Baml.host_callable_tests.call_with_callback(
            callback: { x in "lambda-\(x)" },
            x: 99
        )
        XCTAssertEqual(result, "lambda-99")
    }

    func test_host_callables_async_callable_runs_to_completion() throws {
        let result = try Baml.host_callable_tests.call_with_callback(
            callback: { x in
                await Task.yield()
                return "async-\(x)"
            },
            x: 4
        )
        XCTAssertEqual(result, "async-4")
    }

    func test_host_callables_multiple_callable_keys_are_distinct() throws {
        let counter = LockedBox<[String: Int]>([:])
        let a = try Baml.host_callable_tests.call_with_callback(
            callback: { x in
                counter.with { $0["a", default: 0] += 1 }
                return "a:\(x)"
            },
            x: 1
        )
        let b = try Baml.host_callable_tests.call_with_callback(
            callback: { x in
                counter.with { $0["b", default: 0] += 1 }
                return "b:\(x)"
            },
            x: 2
        )
        XCTAssertEqual(a, "a:1")
        XCTAssertEqual(b, "b:2")
        XCTAssertEqual(counter.snapshot, ["a": 1, "b": 1])
    }

    func test_host_callables_literal_union_callback_states_its_literal_types() throws {
        // `("first" | "second" | int) -> "first" | "second" | int`: Swift
        // spells the literals as `String`, but the callable is stated with
        // the literal union. The return is covariant, so `-> string | int`
        // would not fit the declared callback type.
        let result = try Baml.host_callable_tests.call_literal_union_roundtrip_callback(
            callback: { value in value },
            value: .t0("second")
        )
        XCTAssertEqual(result, .t0("second"))
    }

    func test_host_callables_overlapping_container_union_callback_states_the_selected_list() throws {
        // `int[] | string[]`: the case names the arm, so even an empty list
        // states the arm's exact type.
        let strings = try Baml.host_callable_tests.call_overlapping_container_union_callback(
            callback: { value in value },
            value: .t1(["a", "b"])
        )
        XCTAssertEqual(strings, .t1(["a", "b"]))
        let empty = try Baml.host_callable_tests.call_overlapping_container_union_callback(
            callback: { _ in .t0([]) },
            value: .t1([])
        )
        XCTAssertEqual(empty, .t0([]))
    }

    func test_host_callables_media_callback_states_the_media_type() throws {
        // `(image) -> image`: media is the type `image`, never the stdlib
        // class wrapping it, in the stated function type and on the wire.
        let image = Baml.baml.media.Image(
            _data: try BamlMedia.fromUrl(.image, "https://example.com/a.png", mimeType: "image/png")
        )
        let seen = LockedBox<Int>(0)
        let result = try Baml.host_callable_tests.call_media_roundtrip_callback(
            callback: { value in
                seen.with { $0 += 1 }
                return value
            },
            value: image
        )
        XCTAssertEqual(seen.snapshot, 1)
        XCTAssertEqual(try result.url(), "https://example.com/a.png")
    }

    func test_host_callables_class_callback_round_trips_class_value() throws {
        let result = try Baml.host_callable_tests.call_with_class_callback(
            callback: { p in "\(p.name) is \(p.age)" },
            p: Baml.host_callable_tests.Person(name: "Ada", age: 37)
        )
        XCTAssertEqual(result, "Ada is 37")
    }

    func test_host_callables_call_repeatedly_invokes_callback_n_times() throws {
        let invocations = LockedBox<[Int]>([])
        let results = try Baml.host_callable_tests.call_repeatedly(
            callback: { x in
                invocations.with { $0.append(x) }
                return "item-\(x)"
            },
            n: 5
        )
        XCTAssertEqual(results, ["item-0", "item-1", "item-2", "item-3", "item-4"])
        XCTAssertEqual(invocations.snapshot, [0, 1, 2, 3, 4])
    }

    func test_host_callables_call_repeatedly_with_zero_n_returns_empty_list() throws {
        let invocations = LockedBox<[Int]>([])
        let results = try Baml.host_callable_tests.call_repeatedly(
            callback: { x in
                invocations.with { $0.append(x) }
                return "item-\(x)"
            },
            n: 0
        )
        XCTAssertEqual(results, [])
        XCTAssertEqual(invocations.snapshot, [])
    }

    func test_host_callables_call_with_throwing_in_baml_catches_host_callable_error() throws {
        let result = try Baml.host_callable_tests.call_with_throwing(
            callback: { _ in throw TestError(tag: "boom from host") },
            x: 1
        )
        // BAML's catch reads the host error's class name.
        XCTAssertEqual(result, "caught:TestError")
    }

    func test_host_callables_optional_args_all_unset_apply_host_defaults() throws {
        let result = try Baml.host_callable_tests.call_callback_with_optional_args_all_unset(
            callback: { x, y, z in x * 100 + orDefault(y, 8) * 10 + orDefault(z, 9) },
            x: 5
        )
        XCTAssertEqual(result, [589])
    }

    func test_host_callables_optional_args_partially_set_deliver_by_name() throws {
        let result = try Baml.host_callable_tests.call_callback_with_optional_args_partially_set(
            callback: { x, y, z in x * 100 + orDefault(y, 8) * 10 + orDefault(z, 9) },
            x: 5
        )
        XCTAssertEqual(result, [529, 583])
    }

    func test_host_callables_optional_args_all_set_deliver_both() throws {
        let result = try Baml.host_callable_tests.call_callback_with_optional_args_all_set(
            callback: { x, y, z in x * 100 + orDefault(y, 8) * 10 + orDefault(z, 9) },
            x: 5
        )
        XCTAssertEqual(result, [523])
    }

    func test_host_callable_two_cbs_callbacks_named_f_and_f_returns_both_run() throws {
        // Two callback parameters whose names differ by a `_returns` suffix
        // each keep their own closure: `f(1) + f_returns(2)`.
        let result = try Baml.host_callable_tests.two_cbs(
            f: { x in x * 10 },
            f_returns: { x in x * 100 }
        )
        XCTAssertEqual(result, 210)
    }

    func test_host_callable_hof_receives_a_baml_closure() throws {
        // BAML hands the callback a closure of its own (`x + 1`); it arrives
        // as a callable like any BAML closure returned to Swift, calls back
        // into BAML each time (twice), and its result flows back through
        // `hof`.
        let calls = LockedBox<Int>(0)
        let result = try Baml.host_callable_tests.hof(cb: { f in
            calls.with { $0 += 1 }
            let first = try await f(41)
            let second = try await f(1)
            return first * 100 + second * 2
        })
        XCTAssertEqual(result, 4204)
        XCTAssertEqual(calls.snapshot, 1)
    }

    // SDK_PARITY_LINT(skip): pins a typed SDK's completion of a forwarded panic under a single-class throws
    func test_host_callables_forwarded_panic_stays_a_panic_under_a_declared_throws_class() throws {
        // A panic from a nested BAML call, let escape the callback, is thrown
        // back as the panic it is: under `throws ValidationError` too, since a
        // panic is a panic whatever the callable declares.
        do {
            _ = try Baml.host_callable_tests.call_with_typed_throws_propagating(
                callback: { _ in
                    try await Baml.throws_test.DoPanic_async(message: "forwarded")
                    return "unreachable"
                },
                x: 1
            )
            XCTFail("expected BamlPanic")
        } catch let panic as BamlPanic {
            XCTAssertEqual(panic.className, "baml.panics.UserPanic", panic.message)
            XCTAssertEqual(try panic.value(as: Baml.baml.panics.UserPanic.self).message, "forwarded")
        }
    }

    // SDK_PARITY_LINT(skip): pins a typed SDK's completion of a nested call's cancellation, which it surfaces as its native cancellation
    func test_host_callables_forwarded_async_cancellation_stays_a_panic() throws {
        // A nested async call cancelled by its task surfaces in Swift as a
        // `CancellationError`; let escape the callback, it goes back as the
        // `baml.panics.Cancelled` it stands for, a panic under any declared
        // throws type, never raised as a host exception.
        do {
            _ = try Baml.host_callable_tests.call_with_typed_throws_propagating(
                callback: { _ in
                    let nested = Task { try await Baml.throws_test.SleepMs_async(ms: 2000) }
                    try await Task.sleep(for: .milliseconds(50))
                    nested.cancel()
                    try await nested.value
                    return "unreachable"
                },
                x: 1
            )
            XCTFail("expected BamlPanic")
        } catch let panic as BamlPanic {
            XCTAssertEqual(panic.className, "baml.panics.Cancelled", panic.message)
        }
    }

    // SDK_PARITY_LINT(skip): pins a forwarded BAML error completed as thrown
    func test_host_callables_forwarded_baml_error_is_thrown_as_itself() throws {
        // A nested BAML call's error, let escape the callback, is thrown back
        // as the value it carried, so BAML's typed catch arm matches it; a
        // `HostCallable` wrapping it would not.
        let result = try Baml.host_callable_tests.call_with_typed_throws(
            callback: { x in
                try await Baml.host_callable_tests.call_with_typed_throws_propagating_async(
                    callback: { _ in
                        throw BamlThrownValue(
                            Baml.host_callable_tests.ValidationError(
                                code: 9,
                                message: "from the nested call",
                                fields: []
                            )
                        )
                    },
                    x: x
                )
            },
            x: 1
        )
        XCTAssertEqual(result, "caught: from the nested call")
    }
}
