// The `$runner` typing rule from the appendix (`02_alternatives_considered.md`
// §1): one entry point, and the runner's associated Handle type determines
// what opening a session returns. This file validates that the projection
// typechecks with today's language (generic interface + associated type +
// an `extends` bound at the open site).
//
// The library form of:
//   let s:   Session<T> = F@session(...)                     // Blocking
//   let job: Job<T>     = F@session(..., $runner = jobs)     // Background

interface SessionRunner<Out> {
    type Handle

    function open(self, s: Session<Out, never>) -> Self.Handle throws unknown
}

//# Blocking: the handle IS the session. Kept as a runner only for uniformity.

class BlockingRunner<Out> {
    implements SessionRunner<Out> {
        type Handle = Session<Out, never>

        function open(self, s: Session<Out, never>) -> Session<Out, never> {
            s
        }
    }
}

//# The Job handle: no send lane — a job runs detached, with no caller

//# attached to converse. poll() returns a settlement value, not a throw.

class JobDone<T> {
    result: T,
}

class JobFailed {
    error: string,
    reason: string, // "needs_input" | "error" (02_background_jobs.md)
}

class Job<T> {
    outcome: JobDone<T> | JobFailed,

    function poll(self) -> JobDone<T> | JobFailed {
        self.outcome
    }
}

// A synchronous stand-in for `baml.session.Background`: "background" work
// runs eagerly at open(). The handle type is the point being validated —
// the same open() expression yields Job<T> here and Session<T> above.
class EagerJobRunner<Out> {
    implements SessionRunner<Out> {
        type Handle = Job<Out>

        // A job that stops conversationally settles Failed(needs_input):
        // it runs detached, so nobody is there to answer (02_background_jobs.md).
        function open(self, s: Session<Out, never>) -> Job<Out> {
            let outcome: JobDone<Out> | JobFailed = (
                match (s.run()) {
                    let d: Done<Out> => JobDone { result: d.result },
                    let r: Replied => JobFailed { error: r.message, reason: "needs_input" },
                }
            ) catch_all (e) {
                _ => JobFailed { error: e.to_string(), reason: "error" },
            };
            Job { outcome: outcome }
        }
    }
}

//# The desugar site: `F@session(args, $runner = r)` resolves to the runner's

//# Handle through the bound — R.Handle is the expression's static type.

function open_with<Out, R extends SessionRunner<Out>>(r: R, s: Session<Out, never>) -> R.Handle {
    r.open(s)
}
