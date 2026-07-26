// Optional-argument matrix — port of python_pydantic2
// `test_optional_args.py`.
//
// `BamlOptional` is Swift's spelling of Python's UNSET design:
// `.unset` (default) omits the argument so the engine evaluates the
// BAML default; `nil` (= `.null`) passes an explicit null; `.value(v)`
// passes v. `opt1: int? = 5` has a literal default, `opt2: int? =
// make_opt2()` an engine-evaluated expression default (99).
//
// test_negative_runtime_cases_reject is Python-only — the
// unknown-kwarg / missing-required / duplicate-argument cases are all
// compile errors in Swift, enforced by the generated signature itself.
import XCTest
import Baml

final class TestOptionalArgs: XCTestCase {
    func test_optional_args_runtime_matrix() throws {
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1), [1, 5, 99])
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, opt1: .unset, opt2: .unset), [1, 5, 99])
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, opt1: nil), [1, nil, 99])
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, opt1: .value(8)), [1, 8, 99])
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, opt2: nil), [1, 5, nil])
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, opt2: .value(9)), [1, 5, 9])
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, opt1: nil, opt2: nil), [1, nil, nil])
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, opt1: .value(8), opt2: .value(9)), [1, 8, 9])
    }

    func test_optional_args_unset_and_null_differ_in_one_call() throws {
        // `.unset` means "omit this argument"; `nil` means "pass an
        // explicit null". The two must stay distinct within one call.
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, opt1: .unset, opt2: nil), [1, 5, nil])
        XCTAssertEqual(try Baml.optional_args_probe(arg0: 1, opt1: nil, opt2: .unset), [1, nil, 99])
    }

    func test_optional_args_opt_box_method_matrix() throws {
        let box = try Baml.OptBox.make(base: 10)
        XCTAssertEqual(box.base, 17)

        let box2 = try Baml.OptBox.make(base: 10, opt1: .value(0))
        XCTAssertEqual(box2.base, 10)
        XCTAssertEqual(try box2.probe(arg0: 1), [10, 1, 5])
        XCTAssertEqual(try box2.probe(arg0: 1, opt1: .value(8)), [10, 1, 8])
    }

    func test_optional_args_async_samples() async throws {
        let a = try await Baml.optional_args_probe_async(arg0: 1)
        XCTAssertEqual(a, [1, 5, 99])
        let b = try await Baml.optional_args_probe_async(arg0: 1, opt1: nil)
        XCTAssertEqual(b, [1, nil, 99])
        let c = try await Baml.optional_args_probe_async(arg0: 1, opt2: .value(9))
        XCTAssertEqual(c, [1, 5, 9])
    }
}
