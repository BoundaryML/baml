// "@bammy babysit this PR <url>": from Slack to a merged PR.
//
//   Slack mention  ->  bammy ingress (tools/baml-bench/services/ingress)
//                      inserts a babysit_requests row, acks in the thread
//   this runner    ->  babysit_loop() claims the row, runs merge_issue with
//                      the thread attached, reports the result in it
//
//   cd tools/atb2 && infisical run --projectId <boundary-tools> --env prod -- \
//       baml-cli run -e 'babysit_loop()'                 # serve requests until stopped
//   baml-cli run -e 'babysit_loop(once = true)'          # serve one, then return
//   baml-cli run -e 'babysit("https://github.com/BoundaryML/baml/pull/4715", null)'
//   baml-cli run -e 'request_babysit("https://github.com/BoundaryML/baml/pull/4715")'
//                                                        # queue one by hand (no Slack)
//
// The runner is whatever machine has the toolchain, cargo and a logged-in
// Claude Code CLI; the queue is the store, so the ingress never needs to
// reach it. A claimed request that dies with the runner stays `running`;
// `babysit_loop(reclaim_after_s = ...)` takes those back.

/// One queued request, as the ingress wrote it.
class BabysitRequest {
    id: int,
    pr: string,
    channel: string,
    thread_ts: string,
    requested_by: string?,
    status: string,
}

/// Runs merge_issue on `pr`, narrating in `thread` when there is one:
/// the start, every round (merge_issue posts those), and the result.
function babysit(
    pr: string,
    thread: SlackRef?,
    mode: HandleMode = HandleMode.Live,
    max_rounds: int = 3,
) -> MergeResult {
    tell_thread(
        thread,
        ":eyes: watching "
            + pr
            + " until it merges: CI failures and review comments go back to the agent, up to "
            + max_rounds.to_string()
            + " rounds",
    );
    let r = merge_issue(pr, mode = mode, max_rounds = max_rounds, thread = thread) catch (_) {
        _ => MergeResult { kind: "round_failed", pr: pr, rounds: 0 },
    };
    tell_thread(thread, result_text(r));
    log_event(
        "babysit",
        null,
        null,
        r.to_json(),
        match (thread) {
            null => null,
            let t: SlackRef => t.channel,
        },
        match (thread) {
            null => null,
            let t: SlackRef => t.ts,
        },
    );
    r
}

/// The closing line of a babysit thread.
function result_text(r: MergeResult) -> string {
    let rounds = r.rounds.to_string()
        + (
            if (r.rounds == 1) {
                " round"
            } else {
                " rounds"
            }
        );
    match (r.kind) {
        "merged" => ":white_check_mark: merged after " + rounds + ": " + r.pr,
        "green" => {
            ":large_green_circle: green after "
                + rounds
                + ", nothing left for the agent; waiting on a human to merge "
                + r.pr
        },
        "closed" => ":no_entry_sign: closed without merging: " + r.pr,
        "out_of_rounds" => {
            ":hourglass: out of rounds ("
                + rounds
                + ") with feedback still pending on "
                + r.pr
                + "; a human takes it from here"
        },
        "round_failed" => {
            ":x: a round ended without a push (gate failed or the agent stopped) on "
                + r.pr
                + "; see ~/.atb2/merge for the outcome"
        },
        "waiting_on_checks" => ":hourglass: gave up waiting for checks on " + r.pr,
        "dry_run" => ":test_tube: dry run done on " + r.pr + ", nothing pushed",
        "fork_refused" => {
            ":no_entry: "
                + r.pr
                + " comes from a fork; not run on this host (ATB2_ALLOW_FORKS=1 overrides)"
        },
    }
}

// ==================== The queue ====================

/// Queues a request by hand, the way the ingress does for a mention.
/// `thread` is where to narrate; null = no Slack, the result is only stored.
function request_babysit(pr: string, thread: SlackRef? = null) -> int {
    if (!db_configured()) {
        baml.sys.panic("request_babysit needs the store (FEEDBACK_SUPABASE_URL/KEY)");
    };
    let row = insert(
        "babysit_requests",
        {
            "pr": pr,
            "channel": match (thread) {
                null => "",
                let t: SlackRef => t.channel,
            },
            "thread_ts": match (thread) {
                null => "",
                let t: SlackRef => t.ts,
            },
            "requested_by": "baml run",
            "dataset": dataset(),
        }
            .to_json(),
    );
    baml.json.path_or<int>(row, ".id", 0)
}

/// The oldest queued request, claimed for this runner; null when the queue
/// is empty. The claim is a conditional PATCH (status still queued), so two
/// runners cannot take the same row.
function claim_request(runner: string) -> BabysitRequest? {
    let rows = select(
        "babysit_requests",
        "select=id,pr,channel,thread_ts,requested_by,status&status=eq.queued&order=created_at&limit=1",
    );
    let r = match (rows.at(0)) {
        null => {
            return null;
        },
        let j: json => request_from_row(j),
    };
    let claimed = supabase_send(
        "PATCH",
        "babysit_requests?id=eq." + r.id.to_string() + "&status=eq.queued",
        baml.json.stringify(
            { "status": "running", "claimed_by": runner, "claimed_at": "now()" }.to_json(),
        ),
        "return=representation",
    );
    if (rows_of(claimed).length() == 0) {
        // another runner got it first
        return null;
    };
    r
}

function request_from_row(j: json) -> BabysitRequest {
    BabysitRequest {
        id: baml.json.path_or<int>(j, ".id", 0),
        pr: baml.json.path_or<string>(j, ".pr", ""),
        channel: baml.json.path_or<string>(j, ".channel", ""),
        thread_ts: baml.json.path_or<string>(j, ".thread_ts", ""),
        requested_by: baml.json.path_or<string?>(j, ".requested_by", null),
        status: baml.json.path_or<string>(j, ".status", "queued"),
    }
}

function finish_request(id: int, status: string, result: json) -> null {
    update(
        "babysit_requests",
        "id=eq." + id.to_string(),
        { "status": status, "result": result, "finished_at": "now()" }.to_json(),
    );
    null
}

/// Requests left `running` longer than `after_s` (a runner died) go back to
/// the queue.
function reclaim_stale(after_s: int) -> int {
    let stale = select(
        "babysit_requests",
        "select=id&status=eq.running&claimed_at=lt." + url_encode(iso_seconds_ago(after_s)),
    );
    for (let r in stale) {
        update(
            "babysit_requests",
            "id=eq." + baml.json.path_or<int>(r, ".id", 0).to_string(),
            { "status": "queued", "claimed_by": null, "claimed_at": null }.to_json(),
        );
    }
    stale.length()
}

/// `now - s` as an ISO instant, for a PostgREST timestamp filter.
function iso_seconds_ago(s: int) -> string {
    baml.time.Instant.now().as<baml.ops.Subtract<baml.time.Duration, Output = baml.time.Instant>>
        .sub(baml.time.Duration.from_seconds(s))
        .to_string()
}

/// The thread a request came from, or null when it was queued without Slack.
function request_thread(r: BabysitRequest) -> SlackRef? {
    if (r.channel.length() == 0 || r.thread_ts.length() == 0) {
        null
    } else {
        SlackRef { channel: r.channel, ts: r.thread_ts }
    }
}

/// A PR url in a mention ("babysit this PR https://github.com/o/r/pull/12",
/// "<https://github.com/o/r/pull/12|pull/12>"), or null. Pure; the ingress
/// has the same rule in Python.
function pr_url_in(text: string) -> string? {
    for (
        let raw in text.split(" ").flat_map((t: string) -> string[] {
            t.split("\n")
        })
    ) {
        let tok = raw.trim();
        // Slack wraps links: <url> or <url|label>
        let inner = if (tok.starts_with("<") && tok.includes(">")) {
            tok.slice(1, tok.index_of(">") ?? tok.length())
        } else {
            tok
        };
        let url = inner.split("|").at(0) ?? "";
        let core = trim_punct(url);
        if (core.starts_with("https://github.com/") && core.includes("/pull/")) {
            let n = core.split("/pull/").at(1) ?? "";
            let digits = n.split("/").at(0) ?? "";
            if (
                digits.length() > 0
                    && digits.chars().every((c: string) -> bool {
                        c >= "0" && c <= "9"
                    })
            ) {
                return (core.split("/pull/").at(0) ?? "") + "/pull/" + digits;
            };
        };
    }
    null
}

// ==================== The runner ====================

//#babysit_loop(once: bool = false, mode: HandleMode = HandleMode.Live, reclaim_after_s: int = 21600) -> int
function babysit_loop(
    once: bool = false,
    mode: HandleMode = HandleMode.Live,
    reclaim_after_s: int = 21600,
) -> int {
    if (!db_configured()) {
        baml.sys.panic("babysit_loop needs the store (FEEDBACK_SUPABASE_URL/KEY)");
    };
    let runner = (baml.env.get("HOSTNAME") ?? baml.env.get("USER") ?? "runner") + "/" + baml.id.new().slice(0, 8);
    let served = 0;
    log.info({ "babysit": runner, "polling_s": poll_s(), "once": once });
    while (true) {
        //# stale claims back to the queue
        let back = reclaim_stale(reclaim_after_s);
        if (back > 0) {
            log.warn({ "babysit": runner, "reclaimed": back });
        };
        //# claim the oldest queued request
        match (claim_request(runner)) {
            null => {
                if (once && served > 0) {
                    return served;
                };
                wait_poll();
            },
            let r: BabysitRequest => {
                log.info({ "babysit": runner, "request": r.id, "pr": r.pr, "by": r.requested_by });
                //# run it, narrating in the request's thread
                let result = babysit(r.pr, request_thread(r), mode = mode);
                finish_request(
                    r.id,
                    if (result.kind == "round_failed") {
                        "failed"
                    } else {
                        "done"
                    },
                    result.to_json(),
                );
                served += 1;
                if (once) {
                    return served;
                };
            },
        };
    }
    served
}

// ==================== Unit tests (token-free) ====================

testset "babysit" {
    test "a PR url comes out of a mention, Slack-wrapped or not" {
        assert.equal(
            pr_url_in("babysit this PR https://github.com/BoundaryML/baml/pull/4715") ?? "",
            "https://github.com/BoundaryML/baml/pull/4715",
        );
        assert.equal(
            pr_url_in("babysit <https://github.com/BoundaryML/baml/pull/4715|pull/4715> please")
                ?? "",
            "https://github.com/BoundaryML/baml/pull/4715",
        );
        assert.equal(
            pr_url_in("babysit <https://github.com/BoundaryML/baml/pull/4715/files>") ?? "",
            "https://github.com/BoundaryML/baml/pull/4715",
        );
        assert.equal(
            pr_url_in("babysit https://github.com/BoundaryML/baml/pull/4715.") ?? "",
            "https://github.com/BoundaryML/baml/pull/4715",
        );
        assert.equal(
            pr_url_in("babysit https://github.com/BoundaryML/baml/issues/4715") == null,
            true,
        );
        assert.equal(pr_url_in("hello") == null, true);
    }

    test "the closing line names the outcome" {
        assert.contains(
            result_text(MergeResult { kind: "merged", pr: "https://x/pull/1", rounds: 2 }),
            "merged after 2 rounds",
        );
        assert.contains(
            result_text(MergeResult { kind: "green", pr: "https://x/pull/1", rounds: 1 }),
            "waiting on a human",
        );
        assert.contains(
            result_text(MergeResult { kind: "out_of_rounds", pr: "https://x/pull/1", rounds: 3 }),
            "out of rounds",
        );
        assert.contains(
            result_text(MergeResult { kind: "fork_refused", pr: "https://x/pull/1", rounds: 0 }),
            "fork",
        );
    }

    test "a round line counts what went to the agent" {
        let fb = PrFeedback {
            checks: [CheckRow { name: "clippy", bucket: "fail", link: null }],
            comments: [],
        };
        assert.equal(
            round_text(1, "fixed", fb),
            ":arrows_counterclockwise: round 1: 1 failing check, 0 reviewer comments → fixed, pushed",
        );
        assert.contains(round_text(2, "gate_failed", fb), "nothing pushed");
    }

    test "a request row reads back; a queued-by-hand request has no thread" {
        let r = request_from_row(
            baml.json.parse(
                `{"id":7,"pr":"https://github.com/o/r/pull/1","channel":"","thread_ts":"","requested_by":null,"status":"queued"}`,
            ),
        );
        assert.equal(r.id, 7);
        assert.equal(request_thread(r) == null, true);
        let s = request_from_row(
            baml.json.parse(
                `{"id":8,"pr":"https://github.com/o/r/pull/2","channel":"C1","thread_ts":"1.2","requested_by":"U1","status":"queued"}`,
            ),
        );
        assert.equal(request_thread(s)?.ts ?? "", "1.2");
    }

    test "the stale cutoff is an instant in the past" {
        let cutoff = iso_seconds_ago(60);
        assert.is_true(cutoff.length() > 10);
        assert.is_true(cutoff < baml.time.Instant.now().to_string());
    }
}
