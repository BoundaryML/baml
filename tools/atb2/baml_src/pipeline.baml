// The whole pipeline, end to end, against the store:
//
//   ingest   new reports (PostHog `baml feedback` events, the Slack intake
//            channel) -> feedback rows
//   triage   each new report -> create_issue -> organize_issue -> gauge_issue
//            -> an issues row, and a Slack thread opened for it
//   handle   every open, gauged issue -> handle_issue -> a runs row; the
//            thread hears about the PR, or that a human is needed
//   merge    every in-progress issue with a PR -> merge_issue -> merged
//
//   cd tools/atb2 && baml run -e 'run_pipeline()'
//   baml run -e 'run_pipeline(mode = HandleMode.DryRun, stages = "ingest,triage")'
//
// Every stage is idempotent against the store: reports are keyed by their
// source id, issues by id, and the intakes resume from a cursor. Without
// the store (FEEDBACK_SUPABASE_*) the stages still run, from the intakes
// straight through, and nothing is remembered between runs.

/// What one run did.
class PipelineReport {
    /// New reports written to the store.
    ingested: int,
    /// Issues created from them (reports already fixed on this toolchain
    /// make none).
    issues_created: int,
    /// handle_issue runs that produced an outcome.
    handled: int,
    /// merge_issue calls that ended merged.
    merged: int,
    /// Anything skipped, one line each.
    skipped: string[],
}

//#run_pipeline(mode: HandleMode = HandleMode.Live, stages: string = "ingest,triage,handle,merge") -> PipelineReport
function run_pipeline(
    mode: HandleMode = HandleMode.Live,
    stages: string = "ingest,triage,handle,merge",
) -> PipelineReport {
    let want = stages.split(",").map((s: string) -> string {
        s.trim()
    });
    let report = PipelineReport { ingested: 0, issues_created: 0, handled: 0, merged: 0, skipped: [] };
    if (!db_configured()) {
        report.skipped.push(
            "store: FEEDBACK_SUPABASE_URL/KEY unset; nothing is remembered between runs",
        );
    };
    //# ingest: new reports -> feedback rows
    let fresh: Feedback[] = [];
    if (want.includes("ingest")) {
        fresh = ingest_feedback();
        report.ingested = fresh.length();
    };
    //# triage: report -> issue (create, organize, gauge), thread opened
    if (want.includes("triage")) {
        for (let fb in fresh) {
            match (triage_feedback(fb)) {
                null => report.skipped.push(fb.id + ": no issue (fixed on this toolchain)"),
                let _i: Issue => {
                    report.issues_created += 1;
                    null
                },
            };
        }
    };
    //# handle: open, gauged issues -> handle_issue -> runs row
    if (want.includes("handle")) {
        for (let issue in load_issues_in("open")) {
            match (handle_one(issue, mode)) {
                null => {
                    report.skipped.push(
                        issue.id + ": " + (handle_ready(issue, false) ?? "no outcome"),
                    )
                },
                let _o: HandleOutcome => {
                    report.handled += 1;
                    null
                },
            };
        }
    };
    //# merge: in-progress issues with a PR -> merge_issue
    if (want.includes("merge")) {
        for (let issue in load_issues_in("in_progress")) {
            match (merge_one(issue, mode)) {
                null => report.skipped.push(issue.id + ": no PR to watch"),
                let r: MergeResult => {
                    if (r.kind == "merged") {
                        report.merged += 1;
                    } else {
                        report.skipped.push(issue.id + ": merge_issue ended " + r.kind);
                    };
                },
            };
        }
    };
    log.info(
        {
            "ingested": report.ingested,
            "issues_created": report.issues_created,
            "handled": report.handled,
            "merged": report.merged,
            "skipped": report.skipped.length(),
        },
    );
    report
}

/// run_pipeline forever, `every_s` apart: what the Fly runner's `pipeline`
/// process runs. A run that panics is logged and the loop goes on.
//#pipeline_loop(every_s: int = 300, mode: HandleMode = HandleMode.Live) -> null
function pipeline_loop(every_s: int = 300, mode: HandleMode = HandleMode.Live) -> null {
    while (true) {
        //# one run
        let r = run_pipeline(mode = mode);
        log.info(
            {
                "pipeline_loop": "run done",
                "ingested": r.ingested,
                "handled": r.handled,
                "merged": r.merged,
            },
        );
        //# sleep
        baml.sys.sleep(baml.time.Duration.from_seconds(every_s)) catch (_) {
            _ => null,
        };
    }
    null
}

// ==================== ingest ====================

/// New reports from every intake, written to the store. A report already
/// in the store (same source id) is not returned again.
function ingest_feedback() -> Feedback[] {
    let known = known_feedback_ids();
    let fresh: Feedback[] = [];
    //# PostHog: baml feedback events since the cursor
    let ph = posthog_feedback(get_cursor("posthog_after"));
    for (let fb in ph.feedback) {
        if (!known.includes(fb.id)) {
            save_feedback(fb, origin_of(fb));
            fresh.push(fb);
        };
    }
    match (ph.newest) {
        null => null,
        let n: string => set_cursor("posthog_after", n),
    };
    //# Slack: intake channel messages since the cursor
    let sl = slack_feedback(get_cursor("slack_oldest"));
    for (let fb in sl.feedback) {
        if (!known.includes(fb.id)) {
            save_feedback(fb, origin_of(fb));
            fresh.push(fb);
        };
    }
    match (sl.newest_ts) {
        null => null,
        let n: string => set_cursor("slack_oldest", n),
    };
    log.info(
        {
            "ingested": fresh.length(),
            "posthog": ph.feedback.length(),
            "slack": sl.feedback.length(),
        },
    );
    fresh
}

/// Where a report came from, from its id: PH-<report_id> or SL-<ts>.
function origin_of(fb: Feedback) -> json {
    match (fb.source) {
        FeedbackSource.Slack => slack_origin(fb, slack_intake_channel() ?? ""),
        FeedbackSource.BamlFeedback => {
            ({ "kind": "posthog", "report_id": fb.id.slice(3, fb.id.length()) }).to_json()
        },
        FeedbackSource.Github => ({ "kind": "github", "id": fb.id }).to_json(),
    }
}

// ==================== triage ====================

/// One report through triage. Null when the repro is already fixed on this
/// toolchain (create_issue makes no ticket); the report is marked so.
function triage_feedback(fb: Feedback) -> Issue? {
    //# create: repro, run it, write the ticket
    let created = create_issue(fb);
    match (created) {
        null => {
            log_event("no_issue", null, fb.id, { "reason": "fixed on this toolchain" }.to_json(), null, null);
            null
        },
        let issue: Issue => {
            //# organize + gauge
            let gauged = gauge_issue(organize_issue(issue));
            save_issue(gauged);
            //# link the report to its issue
            save_feedback(with_issue(fb, gauged.id), origin_of(fb));
            //# open the issue's thread
            notify("issue_created", gauged, issue_created_text(gauged), { "feedback": fb.id }.to_json());
            gauged
        },
    }
}

function with_issue(fb: Feedback, issue_id: string) -> Feedback {
    Feedback {
        id: fb.id,
        title: fb.title,
        body: fb.body,
        source: fb.source,
        author: fb.author,
        toolchain: fb.toolchain,
        files: fb.files,
        comments: fb.comments,
        issue_ids: if (fb.issue_ids.includes(issue_id)) {
            fb.issue_ids
        } else {
            fb.issue_ids.concat([issue_id])
        },
    }
}

// ==================== handle ====================

/// handle_issue for one issue, its outcome saved and told. Null when
/// handle_issue did not run (not ready, already fixed).
function handle_one(issue: Issue, mode: HandleMode) -> HandleOutcome? {
    //# a stale outcome.json from an earlier run must not be read as this run's
    let stale = run_dir_for(branch_for(issue)) + "/outcome.json";
    if (baml.fs.exists(stale)) {
        baml.fs.remove(stale) catch (_) {
            _ => null,
        };
    };
    let after = handle_issue(issue, mode = mode);
    let o = read_outcome(issue);
    match (o) {
        null => null,
        let out: HandleOutcome => {
            save_run(after, out, mode);
            save_issue(after);
            tell_outcome(after, out, mode);
            out
        },
    }
}

/// The thread hears how the run ended.
function tell_outcome(issue: Issue, o: HandleOutcome, mode: HandleMode) -> null {
    match (o.kind) {
        "fixed" => {
            if (mode == HandleMode.DryRun) {
                notify(
                    "fixed_dry_run",
                    issue,
                    ":test_tube: dry run: the fix passed the gate on `"
                        + (o.branch ?? "?")
                        + "`, nothing pushed",
                    o.to_json(),
                )
            } else {
                notify("pr_opened", issue, pr_opened_text(issue, o), o.to_json())
            }
        },
        _ => notify("needs_human", issue, needs_human_text(issue, o), o.to_json()),
    }
}

// ==================== merge ====================

/// merge_issue for one in-progress issue. Null when it has no PR.
function merge_one(issue: Issue, mode: HandleMode) -> MergeResult? {
    let pr = match (issue.status) {
        let s: InProgress => s.pr,
        _ => null,
    };
    match (pr) {
        null => null,
        let url: string => {
            let r = merge_issue(url, mode = mode, issue_id = issue.id);
            if (r.kind == "merged") {
                let merged = with_status(issue, Merged { state: "merged", pr: url });
                save_issue(merged);
                notify("merged", merged, merged_text(merged, url), r.to_json());
            } else {
                log_event("merge_issue", issue.id, null, r.to_json(), null, null);
            };
            r
        },
    }
}

// ==================== Unit tests (token-free) ====================

testset "pipeline" {
    test "a report is linked to its issue once" {
        let fb = with_issue(with_issue(sample_feedback(), "ISSUE-1"), "ISSUE-1");
        assert.equal(fb.issue_ids, ["ISSUE-1"]);
        assert.equal(fb.title, sample_feedback().title);
    }

    test "origins follow the id" {
        assert.equal(
            baml.json.path_or<string>(origin_of(sample_feedback()), ".report_id", ""),
            "abc",
        );
        let sl = feedback_from_slack(baml.json.parse(`{"type":"message","user":"U1","ts":"1.000200","text":"x"}`), "C1")
            ?? baml.sys.panic("no feedback");
        assert.equal(baml.json.path_or<string>(origin_of(sl), ".kind", ""), "slack");
    }

    test "without the store or intakes a run does nothing and says so" {
        if (!db_configured() && !posthog_configured() && slack_intake_channel() == null) {
            let r = run_pipeline(mode = HandleMode.DryRun, stages = "ingest,triage");
            assert.equal(r.ingested, 0);
            assert.equal(r.issues_created, 0);
            assert.is_true(r.skipped.length() >= 1);
        };
    }

    test "stages are picked by name" {
        if (!db_configured()) {
            let r = run_pipeline(mode = HandleMode.DryRun, stages = "merge");
            assert.equal(r.merged, 0);
            assert.equal(r.ingested, 0);
        };
    }
}
