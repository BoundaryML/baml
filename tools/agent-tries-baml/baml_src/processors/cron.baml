// Cron — the daily driver. Not a claim processor: a timer loop that periodically
// generates fresh hard benchmark tasks (via the GenerateTasks LLM function) and
// enqueues them, plus the changelog release poller (ports services/cron +
// the bench_core.changelog_sync periodic sweep).
//
// Focused port: the baml-version refresh and cohort fast-fan-in reconciler are
// deferred; task generation + changelog polling are the live actions.

class CronTaskDoc {
    source: string,
    prompt: string,
    status: string,
}

function run_cron(svc: ServiceClient, interval_ms: int, gen_count: int, generate: bool) -> int {
    while (true) {
        if (generate) {
            // Recently-run tasks would be rendered here to avoid repeats; "" for now.
            let gen = GenerateTasks("", gen_count) catch (_) {
                _ => TaskList { tasks: [] }
            };
            for (let prompt in gen.tasks) {
                let doc = baml
                    .json
                    .to_string(CronTaskDoc { source: "cron", prompt: prompt, status: "queued" }) catch (_) {
                    _ => ""
                };
                if (doc != "") {
                    let _ = svc.create("tasks", doc) catch (_) {
                        _ => ""
                    };
                }
            }
        }
        let _ = baml.sys.sleep(baml.time.Duration.from_milliseconds(interval_ms)) catch (_) {
            _ => null
        };
    }
    return 0;
}

// Changelog release poller: every CHANGELOG_POLL_SECS, enqueue a queued entry
// for any GitHub release without one (sync_changelog is idempotent by version;
// the changelog_worker on the service generates them). Detached loop — swallows
// every error (BEP-034) and never exits.
function run_changelog_poll(interval_ms: int) -> int {
    baml.io.println("changelog poller starting (every " + baml.unstable.string(interval_ms) + "ms)");
    while (true) {
        let n = sync_changelog() catch (_) {
            _ => 0
        };
        if (n > 0) {
            baml.io.println("changelog poll: enqueued " + baml.unstable.string(n) + " release(s)");
        }
        let _ = baml.sys.sleep(baml.time.Duration.from_milliseconds(interval_ms)) catch (_) {
            _ => null
        };
    }
    return 0;
}

function changelog_poll_enabled() -> bool {
    return env_bool("CHANGELOG_POLL_ENABLED", false);
}

function changelog_poll_secs() -> int {
    return env_int("CHANGELOG_POLL_SECS", 600);
}
