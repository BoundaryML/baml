// Cancellation — port of python_pydantic2 `test_cancellation.py`.
//
// Swift surface: Task cancellation → `cancel_function_call` → the
// engine's `baml.panics.Cancelled` panic → Swift `CancellationError`
// (Python's asyncio.CancelledError analog). The BamlCallContext /
// ctx.abort() cases are not ported — Swift has no call-context surface
// yet (structured concurrency owns cancellation); sync cancellation
// does not exist in either bridge.
import XCTest
import Baml
import BamlBridge

private let maxCancellationSeconds = 0.5

final class TestCancellation: XCTestCase {
    func test_cancellation_sync_call_returns_none() throws {
        try Baml.throws_test.SleepMs(ms: 1)
    }

    func test_cancellation_async_call_returns_none() async throws {
        try await Baml.throws_test.SleepMs_async(ms: 1)
    }

    func test_cancellation_async_cancel_via_task_cancel() async throws {
        let start = ContinuousClock.now
        let task = Task {
            try await Baml.throws_test.SleepMs_async(ms: 2000)
        }
        try await Task.sleep(for: .milliseconds(50))
        task.cancel()
        do {
            _ = try await task.value
            XCTFail("expected CancellationError")
        } catch is CancellationError {
            // expected
        }
        let elapsed = ContinuousClock.now - start
        XCTAssertLessThan(elapsed, .seconds(maxCancellationSeconds), "cancellation was not fast")
    }

    func test_cancellation_async_cancel_via_timeout_race() async throws {
        // The asyncio.wait_for analog: race the call against a timeout
        // in a task group; the loser is cancelled.
        let start = ContinuousClock.now
        do {
            try await withThrowingTaskGroup(of: Void.self) { group in
                group.addTask {
                    try await Baml.throws_test.SleepMs_async(ms: 2000)
                }
                group.addTask {
                    try await Task.sleep(for: .milliseconds(50))
                    throw TimeoutRace()
                }
                defer { group.cancelAll() }
                try await group.next()
            }
            XCTFail("expected TimeoutRace")
        } catch is TimeoutRace {
            // expected — and the BAML task must have cancelled fast.
        } catch is CancellationError {
            // acceptable ordering: the group surfaced the cancelled call.
        }
        let elapsed = ContinuousClock.now - start
        XCTAssertLessThan(elapsed, .seconds(maxCancellationSeconds), "cancellation was not fast")
    }
}

private struct TimeoutRace: Error {}
