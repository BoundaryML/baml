// Worker-pool primitives. Two independent pools, one per model tier, so cheap
// Haiku waves never queue behind Sonnet judgments:
//   FAST_CONCURRENCY  — intention inference (GraderFast), default 24
//   JUDGE_CONCURRENCY — grading/refutation (Grader), default 24
//
// The shared queue counter is safe because green threads are cooperative:
// take() has no await point, so its read-increment is atomic. The
// content-addressed cache makes scheduling irrelevant to determinism: keys
// depend only on inputs, and all aggregation happens on sorted collections.

class WorkQueue {
    next: int,

    // Claim the next index below `limit`; -1 when the queue is drained.
    // No await inside -> atomic under cooperative scheduling.
    function take(self, limit: int) -> int {
        if (self.next >= limit) {
            return -1;
        }
        let i = self.next;
        self.next = self.next + 1;
        i
    }
}

function pool_size(env_name: string, default: int) -> int {
    let raw = baml.env.get(env_name) catch (_) {
        _ => null
    };
    let n = match (raw) {
        let v: string => {
            baml.json.from_string<int>(v) catch (_) {
                _ => default
            }
        },
        null => default,
    };
    n.clamp(1, 64)
}

function fast_concurrency() -> int {
    pool_size("FAST_CONCURRENCY", 24)
}

function judge_concurrency() -> int {
    pool_size("JUDGE_CONCURRENCY", 24)
}
