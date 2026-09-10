// The whole pipeline, end to end, against the store:
//
//   ingest   new reports (PostHog `baml feedback` events, the Slack intake
//            channel) -> feedback rows
//   triage   each new report -> create_issue -> organize_issue -> gauge_issue
//            -> an issues row, and a Slack thread opened for it
//   verify   open issues re-run on the runner's toolchain (the latest
//            nightly); the ones that pass ship, see verify_issue.baml
//   handle   every approved, gauged issue -> handle_issue -> a runs row; the
//            thread hears about the PR, or that a human is needed
//   merge    reconcile issue PRs with the shared merge_issue babysitter queue
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
    /// Issues re-verified on the current toolchain that now pass (verify).
    verified_fixed: int,
    /// handle_issue runs that produced an outcome.
    handled: int,
    /// merge_issue calls that ended merged.
    merged: int,
    /// Anything skipped, one line each.
    skipped: string[],
}

//#run_pipeline(mode: HandleMode = HandleMode.Live, stages: string = "ingest,triage,verify,handle") -> PipelineReport
function run_pipeline(
    mode: HandleMode = HandleMode.Live,
    stages: string = "ingest,triage,verify,handle",
    github_ingress: bool = true,
) -> PipelineReport {
    let want = stages.split(",").map((s: string) -> string {
        s.trim()
    });
    let report = PipelineReport {
        ingested: 0,
        issues_created: 0,
        verified_fixed: 0,
        handled: 0,
        merged: 0,
        skipped: [],
    };
    if (!db_configured()) {
        report.skipped.push(
            "store: FEEDBACK_SUPABASE_URL/KEY unset; nothing is remembered between runs",
        );
    };
    //# ingest: new reports -> feedback rows
    let fresh: Feedback[] = [];
    if (want.includes("ingest")) {
        fresh = ingest_feedback(github_ingress = github_ingress);
        report.ingested = fresh.length();
    };
    //# triage: report -> issue (create, organize, gauge), thread opened
    // without the store the issues live only here, and flow straight on to
    // the later stages of this same run
    let created: Issue[] = [];
    if (want.includes("triage")) {
        for (
            let fb in if (db_configured()) {
                untriaged_feedback()
            } else {
                fresh
            }
        ) {
            match (triage_feedback(fb)) {
                null => report.skipped.push(fb.id + ": no issue (fixed or needs report details)"),
                let i: Issue => {
                    report.issues_created += 1;
                    created.push(i);
                    // Start the first fix before triaging another report. The separate
                    // GitHub poller continues storing arrivals while this agent works.
                    if (db_configured() && want.includes("handle") && issue_auto_ready(i)) {
                        let outcome = handle_one(i, mode);
                        if (outcome != null) {
                            report.handled += 1;
                        };
                    };
                    null
                },
            };
        }
    };
    if (want.includes("triage") && db_configured()) {
        announce_open_issues();
    };
    //# verify: open issues re-run on the runner's toolchain; passing ones ship
    if (want.includes("verify") && db_configured()) {
        let v = verify_issues();
        report.verified_fixed = v.shipped.length();
        for (let s in v.skipped) {
            report.skipped.push(s);
        }
    };
    //# handle: approved, gauged issues -> handle_issue -> runs row
    let handled: Issue[] = [];
    if (want.includes("handle")) {
        let open = if (db_configured()) {
            load_issues_in("open")
        } else {
            created
        };
        for (let issue in open) {
            match (handle_one(issue, mode)) {
                null => {
                    report.skipped.push(
                        issue.id + ": " + (handle_ready(issue, false) ?? "no outcome"),
                    )
                },
                let o: HandleOutcome => {
                    report.handled += 1;
                    match (o.pr) {
                        null => null,
                        let pr: string => {
                            handled.push(
                                with_status(issue, InProgress { state: "in_progress", pr: pr }),
                            )
                        },
                    };
                    null
                },
            };
        }
    };
    log.info(
        {
            "ingested": report.ingested,
            "issues_created": report.issues_created,
            "verified_fixed": report.verified_fixed,
            "handled": report.handled,
            "merged": report.merged,
            "skipped": report.skipped.length(),
        },
    );
    report
}

/// The report of a run that ended in a store, intake or Slack failure.
function failed_run(what: string) -> PipelineReport {
    log.error({ "pipeline_loop": "run failed", "at": what });
    PipelineReport {
        ingested: 0,
        issues_created: 0,
        verified_fixed: 0,
        handled: 0,
        merged: 0,
        skipped: [what + " failed; next run retries"],
    }
}

/// run_pipeline forever, `every_s` apart: what the Fly runner runs. A store,
/// intake or Slack failure ends the run, is logged, and the next run
/// retries; every stage is idempotent against the store. Anything else
/// (a panic) ends the process and the Fly restart policy brings it back.
//#pipeline_loop(every_s: int = 300, mode: HandleMode = HandleMode.Live) -> null
function pipeline_loop(
    every_s: int = 300,
    mode: HandleMode = HandleMode.Live,
    github_ingress: bool = true,
) -> null {
    while (true) {
        //# one run; a store, intake or Slack failure ends this run only
        if (mode == HandleMode.Live && db_configured()) {
            null;
        };
        let r = run_pipeline(mode = mode, github_ingress = github_ingress) catch (e) {
            StoreError => failed_run("store"),
            IntakeError => failed_run("intake"),
            SlackError => failed_run("slack"),
        };
        log.info(
            {
                "pipeline_loop": "run done",
                "ingested": r.ingested,
                "handled": r.handled,
                "merged": r.merged,
                "skipped": r.skipped,
            },
        );
        //# sleep
        baml.sys.sleep(baml.time.Duration.from_seconds(every_s)) catch (_) {
            _ => null,
        };
    }
    null
}

/// The Fly runner's one process: the request loop in the background, the
/// pipeline loop in the foreground, one machine and one volume between them.
//#runner_loop(every_s: int = 300, mode: HandleMode = HandleMode.Live) -> null
function runner_loop(every_s: int = 300, mode: HandleMode = HandleMode.Live) -> null {
    if (mode == HandleMode.Live) {
        spawn { agent_turn_loop() };
        spawn { github_intake_loop() };
    };
    pipeline_loop(every_s = every_s, mode = mode, github_ingress = mode != HandleMode.Live)
}

// ==================== ingest ====================

/// New reports from every intake, written to the store. A report already
/// in the store (same source id) is not returned again.
function ingest_feedback(github_ingress: bool = true) -> Feedback[] {
    let known = known_feedback_ids();
    // GitHub runs first for one-shot invocations. The live runner has a dedicated
    // poller, so a long agent turn never delays reports reaching the feedback store.
    let github: Feedback[] = if (github_ingress) {
        ingest_github_issues()
    } else {
        []
    };
    let fresh: Feedback[] = [];
    for (let fb in github) {
        fresh.push(fb);
    }
    //# PostHog: baml feedback events since the cursor (or the resumed scan)
    let ph = posthog_feedback(get_cursor("posthog_after"), resume = get_cursor("posthog_resume")) catch (_) {
        IntakeError => {
            log.warn({ "intake": "posthog", "result": "unavailable; cursor retained" });
            PosthogFeedback { feedback: [], newest: null, resume: null, ran: false }
        },
    };
    for (let fb in ph.feedback) {
        if (!known.includes(fb.id)) {
            save_feedback(fb, origin_of(fb));
            log_event(
                "ingested",
                null,
                fb.id,
                { "summary": "Report received.", "source": fb.source.to_string() }.to_json(),
                null,
                null,
            );
            fresh.push(fb);
        };
    }
    if (ph.ran) {
        advance_cursor("posthog", ph.newest, ph.resume);
    };
    //# Slack: intake channel messages since the cursor (or the resumed scan)
    let sl = slack_feedback(get_cursor("slack_oldest"), resume = get_cursor("slack_resume")) catch (_) {
        SlackError => {
            log.warn({ "intake": "slack", "result": "unavailable; cursor retained" });
            SlackFeedback { feedback: [], newest_ts: null, resume: null, ran: false }
        },
    };
    for (let fb in sl.feedback) {
        if (!known.includes(fb.id)) {
            save_feedback(fb, origin_of(fb));
            log_event(
                "ingested",
                null,
                fb.id,
                { "summary": "Report received.", "source": fb.source.to_string() }.to_json(),
                null,
                null,
            );
            fresh.push(fb);
        };
    }
    if (sl.ran) {
        advance_cursor("slack", sl.newest_ts, sl.resume);
    };
    log.info(
        {
            "github": github.length(),
            "ingested": fresh.length(),
            "posthog": ph.feedback.length(),
            "slack": sl.feedback.length(),
        },
    );
    fresh
}

/// Moves an intake's cursor only once its scan is complete; the caller
/// skips this entirely when the intake did not run (unconfigured), so a
/// stored continuation survives until the intake is back. While a scan
/// is cut short by the page cap, the continuation and the newest timestamp
/// seen so far are kept instead, and the next run picks up where it left
/// off; the timestamp cursor moves when the last page is in. Both intakes
/// return newest first, so moving it early would skip the older pages.
function advance_cursor(intake: string, newest: string?, resume: string?) -> null {
    let pending = get_cursor(intake + "_pending");
    let high = match (newest) {
        null => pending,
        let n: string => {
            if (pending == null || n > (pending ?? "")) {
                n
            } else {
                pending
            }
        },
    };
    match (resume) {
        let r: string => {
            set_cursor(intake + "_resume", r);
            match (high) {
                null => null,
                let h: string => set_cursor(intake + "_pending", h),
            };
            log.info({ "intake": intake, "scan": "continues next run" });
        },
        null => {
            match (high) {
                null => null,
                let h: string => {
                    set_cursor(
                        if (intake == "slack") {
                            "slack_oldest"
                        } else {
                            "posthog_after"
                        },
                        h,
                    )
                },
            };
            set_cursor(intake + "_resume", "");
            set_cursor(intake + "_pending", "");
        },
    };
    null
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

/// Includes durable Slack events and reports ingested in earlier runs. Scan
/// past no_issue rows so they cannot starve newer reports at the head of a page.
function untriaged_feedback() -> Feedback[] {
    let pending: Feedback[] = [];
    let offset = 0;
    while (pending.length() < 50) {
        let rows = select(
            "feedback",
            "select=*&issue_ids=eq.%7B%7D&dataset="
                + eq(dataset())
                + "&order=received_at,id&limit=100&offset="
                + offset.to_string(),
        );
        for (let row in rows) {
            let id = baml.json.path_or<string>(row, ".id", "");
            if (
                select(
                    "events",
                    "select=id&feedback_id="
                        + eq(id)
                        + "&kind=eq.no_issue&dataset="
                        + eq(dataset())
                        + "&limit=1",
                )
                    .length()
                    == 0
            ) {
                pending.push(feedback_from_row(row));
                if (pending.length() >= 50) {
                    return pending;
                };
            };
        }
        if (rows.length() < 100) {
            return pending;
        };
        offset += 100;
    }
    pending
}

// ==================== triage ====================

/// One report through triage. Insufficient reports and already-fixed repros
/// get a terminal no_issue event instead of a ticket or Slack announcement.
function triage_feedback(fb: Feedback) -> Issue? {
    //# create: repro, run it, write the ticket
    log_event(
        "enrich_started",
        null,
        fb.id,
        { "summary": "Checking report quality and reproducing the reported behavior." }.to_json(),
        null,
        null,
    );
    let created = create_issue(fb) catch (e) {
        FeedbackNeedsDetails => {
            // Keep the original report, but do not create an issue or page a shepherd.
            // This terminal event also prevents the same report being retried every poll.
            log_event("no_issue", null, fb.id, { "reason": "needs_details" }.to_json(), null, null);
            return null;
        },
    };
    match (created) {
        null => {
            log_event("no_issue", null, fb.id, { "reason": "fixed on this toolchain" }.to_json(), null, null);
            null
        },
        let issue: Issue => {
            let duplicate = duplicate_issue(issue);
            if (duplicate != null) {
                let merged = merge_duplicate(duplicate ?? baml.sys.panic("checked duplicate"), issue);
                update(
                    "issues",
                    "id=" + eq(merged.id) + "&dataset=" + eq(dataset()),
                    { "repros": merged.repros, "feedback_ids": merged.feedback_ids }.to_json(),
                );
                if (db_configured()) {
                    update(
                        "feedback",
                        "id=" + eq(fb.id) + "&dataset=" + eq(dataset()),
                        { "issue_ids": with_issue(fb, merged.id).issue_ids }.to_json(),
                    );
                };
                notify(
                    "feedback_linked",
                    merged,
                    "Added a related feedback report and its repros to this issue.",
                    { "feedback": fb.id }.to_json(),
                );
                return merged;
            };
            //# organize + gauge
            save_issue(issue);
            log_event(
                "enriched",
                issue.id,
                fb.id,
                {
                    "summary": "Report enriched with a minimal repro and observed behavior.",
                    "repros": issue.repros.length(),
                }
                    .to_json(),
                null,
                null,
            );
            let gauged = with_status(gauge_issue(organize_issue(issue)), Open { state: "open", automatic_fix: true });
            save_issue(gauged);
            log_event(
                "organized",
                gauged.id,
                fb.id,
                {
                    "summary": "Assigned subsystem, shepherd and difficulty.",
                    "subsystem": gauged.subsystem.to_string(),
                    "shepherd": gauged.shepherd,
                    "difficulty": gauged.difficulty,
                }
                    .to_json(),
                null,
                null,
            );
            //# link the report to its issue
            if (db_configured()) {
                update(
                    "feedback",
                    "id=" + eq(fb.id) + "&dataset=" + eq(dataset()),
                    { "issue_ids": with_issue(fb, gauged.id).issue_ids }.to_json(),
                );
            };
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
    if (!issue_auto_ready(issue) || handle_ready(issue, false) != null) {
        return null;
    };
    if (mode == HandleMode.Live && db_configured()) {
        let claimed = rows_of(
            supabase_send(
                "PATCH",
                "issues?id="
                    + eq(issue.id)
                    + "&dataset="
                    + eq(dataset())
                    + "&state=eq.open&status->>automatic_fix=eq.true",
                baml.json.stringify(
                    { "status": InProgress { state: "in_progress", pr: null } }.to_json(),
                ),
                "return=representation",
            ),
        );
        if (claimed.length() != 1) {
            return null;
        };
    };
    //# a stale outcome.json from an earlier run must not be read as this run's
    let stale = run_dir_for(branch_for(issue)) + "/outcome.json";
    if (baml.fs.exists(stale)) {
        baml.fs.remove(stale) catch (_) {
            _ => null,
        };
    };
    notify(
        "fix_started",
        issue,
        "Creating a fix for *" + issue.title + "*; preparing the PR.",
        baml.json.parse("{}"),
    );
    let after = handle_issue(issue, mode = mode, thread = issue_thread(issue.id));
    let o = read_outcome(issue);
    match (o) {
        null => {
            if (mode == HandleMode.Live && !issue_cancelled(issue.id)) {
                save_active_issue(
                    with_status(
                        after,
                        Deferred {
                            state: "deferred",
                            reason: "No fix outcome was produced; inspect the investigation before retrying.",
                            workaround: null,
                        },
                    ),
                );
                log_event(
                    "needs_human",
                    issue.id,
                    null,
                    { "summary": "No fix outcome was produced; automatic retry paused." }.to_json(),
                    null,
                    null,
                );
            };
            null
        },
        let out: HandleOutcome => {
            save_run(after, out, mode);
            if (mode == HandleMode.Live && issue_cancelled(issue.id)) {
                return out;
            };
            if (mode == HandleMode.Live) {
                save_active_issue(after);
            } else {
                save_issue(after);
            };
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

/// New issues run automatically. Historical pending work requires explicit requeue.
function issue_auto_ready(issue: Issue) -> bool {
    match (issue.status) {
        let s: Open => s.automatic_fix == true,
        _ => false,
    }
}

function announce_open_issues() -> null {
    for (let i in load_issues_in("open")) {
        if (issue_auto_ready(i) && i.slack_ts == null) {
            notify(
                "issue_created",
                i,
                issue_created_text(i),
                { "summary": "Issue created; preparing a fix." }.to_json(),
            );
        };
    }
    null
}

function issue_cancelled(id: string) -> bool {
    if (!db_configured()) {
        return false;
    };
    let current = load_issue(id);
    current == null || current?.status is Cancelled
}

/// A late agent result must not overwrite cancellation.
function save_active_issue(issue: Issue) -> null {
    if (!db_configured()) {
        return null;
    };
    update(
        "issues",
        "id=" + eq(issue.id) + "&dataset=" + eq(dataset()) + "&state=neq.cancelled",
        { "status": issue.status, "design_doc": issue.design_doc }.to_json(),
    );
    null
}

/// Only controller credentials enter the root-owned release indexer, never an agent.

testset "automatic_issue_fixes" {
    test "new automatic issues run but historical and cancelled work do not" {
        let issue = issue_in(Subsystem.Compiler, "owner");
        assert.equal(
            issue_auto_ready(with_status(issue, Open { state: "open", automatic_fix: true })),
            true,
        );
        assert.equal(issue_auto_ready(with_status(issue, Open { state: "open" })), false);
        assert.equal(
            issue_auto_ready(with_status(issue, AwaitingApproval { state: "awaiting_approval" })),
            false,
        );
        assert.equal(
            issue_auto_ready(with_status(issue, Cancelled { state: "cancelled", reason: "stop", by: "U1" })),
            false,
        );
        assert.is_true(compact_issue_text("x".repeat(900), 450).length() <= 450);
    }
}
