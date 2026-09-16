// Resolve the latest published nightly for each verification pass. Reuse the
// immutable versioned CLI for all issues in that pass; no controller restart
// is needed when a newer nightly is published.
//
//   baml run -e 'verify_issues()'
//   baml run -e 'verify_issue(load_issue("ISSUE-…") ?? baml.sys.panic("no such issue"))'
//
// Cheap by construction: an issue records the toolchain it was last seen
// broken on (`Issue.version`), and is only re-run when the runner's
// toolchain differs. Every repro runs out of process (check_on_canary), so
// a repro that crashes the compiler cannot take the runner down.

/// What one verification pass did.
class VerifyReport {
    /// The toolchain every issue was checked on.
    toolchain: string,
    /// Issues whose repros were re-run.
    checked: int,
    /// "<issue id>" for each issue that now passes every repro.
    shipped: string[],
    /// "<issue id>: <reason>" for each issue left as it was.
    skipped: string[],
}

/// How an issue fared on the current toolchain.
enum VerifyOutcome {
    /// Every repro passes: the bug is gone.
    Fixed,
    /// At least one repro still fails.
    StillBroken,
    /// Nothing failed, but not every repro could be checked.
    Inconclusive,
}

/// Fold per-repro verdicts into one (pure, unit-tested): fixed only when
/// every repro is Fixed; any StillBroken wins over Inconclusive.
function fold_verdicts(verdicts: Verdict[]) -> VerifyOutcome {
    if (verdicts.length() == 0) {
        return VerifyOutcome.Inconclusive;
    };
    let broken = verdicts.filter((v: Verdict) -> bool {
        v == Verdict.StillBroken
    });
    if (broken.length() > 0) {
        return VerifyOutcome.StillBroken;
    };
    let fixed = verdicts.filter((v: Verdict) -> bool {
        v == Verdict.Fixed
    });
    if (fixed.length() == verdicts.length()) {
        VerifyOutcome.Fixed
    } else {
        VerifyOutcome.Inconclusive
    }
}

/// States worth re-checking: everything before a PR merged.
function verifiable_states() -> string[] {
    ["open", "awaiting_approval", "approved", "in_progress"]
}

/// Whether a pass should re-run this issue on `toolchain` (pure).
function needs_verification(issue: Issue, toolchain: string) -> string? {
    if (issue.repros.length() == 0) {
        return "no repros to run";
    };
    if (toolchain == "unknown") {
        return "runner toolchain unknown";
    };
    if (issue.version == toolchain) {
        return "already confirmed broken on " + toolchain;
    };
    // An inconclusive run leaves `version` alone; without this an issue whose
    // repros need a human would be re-run and re-logged every five minutes.
    if (already_verified(issue.id, toolchain)) {
        return "already verified on " + toolchain;
    };
    null
}

/// A re-verify decision for this issue on this toolchain is already recorded.
function already_verified(issue_id: string, toolchain: string) -> bool {
    if (!db_configured()) {
        return false;
    };
    select(
        "events",
        "select=id&issue_id="
            + eq(issue_id)
            + "&kind=eq.decision&payload-%3E%3Estep="
            + eq("Re-verify on " + toolchain)
            + "&dataset="
            + eq(dataset())
            + "&limit=1",
    )
        .length()
        > 0
}

/// Today, as the Shipped status records it.
function today() -> string {
    baml.time.ZonedDateTime.now_in("UTC").to_string().slice(0, 10)
}

/// Re-run one issue's repros on the runner's toolchain. Fixed: the issue
/// becomes Shipped on that version, `fixed_in` is set for the reporters'
/// CLI, and the thread hears. StillBroken: the issue's version advances so
/// the next pass skips it. Inconclusive: nothing changes.
function verify_issue(
    issue: Issue,
    toolchain: string? = null,
    cli: string? = null,
) -> VerifyOutcome {
    let nightly = if (cli == null) {
        latest_nightly_cli()
    } else {
        VerifiedNightlyCli { path: cli ?? "", version: toolchain ?? "" }
    };
    let current = nightly.version;
    //# run every repro out of process on the current toolchain
    let verdicts: Verdict[] = [];
    let i = 0;
    for (let r in issue.repros) {
        i += 1;
        verdicts.push(check_on_canary(r, i, cli = nightly.path));
    }
    let outcome = fold_verdicts(verdicts);
    log.info({ "issue": issue.id, "verify": outcome.to_string(), "toolchain": current });
    if (issue.version != current) {
        decide(
            "Re-verify on " + current,
            issue.id,
            null,
            match (outcome) {
                VerifyOutcome.Fixed => "Fixed: every repro passes, the issue ships on " + current,
                VerifyOutcome.StillBroken => "Still broken on " + current,
                VerifyOutcome.Inconclusive => "Inconclusive: no repro gave a verdict",
            },
            match (outcome) {
                VerifyOutcome.Fixed => "All " + verdicts.length().to_string() + " repros now match their expectation on this nightly; reporters see it as fixed in this version.",
                VerifyOutcome.StillBroken => "At least one repro still violates its expectation; the issue's toolchain advances so the next pass skips this nightly.",
                VerifyOutcome.Inconclusive => "The repros need manual inspection or the run itself failed, so nothing changes.",
            },
            evidence = {
                "previous_version": issue.version.to_json(),
                "verdicts": verdicts
                    .map((v: Verdict) -> string {
                        v.to_string()
                    })
                    .to_json(),
            },
        );
    };
    match (outcome) {
        VerifyOutcome.Fixed => {
            //# every repro passes: ship it on this toolchain
            let shipped = with_status(issue, Shipped { state: "shipped", version: current, date: today() });
            save_issue(shipped);
            if (db_configured()) {
                update(
                    "issues",
                    "id=" + eq(issue.id) + "&dataset=" + eq(dataset()),
                    { "fixed_in": current }.to_json(),
                );
            };
            notify(
                "issue_shipped",
                shipped,
                issue_shipped_text(shipped, current, reported = reported_versions(issue)),
                { "toolchain": current, "repros": verdicts.length() }.to_json(),
            );
        },
        VerifyOutcome.StillBroken => {
            //# still broken: remember the toolchain so the next pass skips it
            if (db_configured()) {
                update(
                    "issues",
                    "id=" + eq(issue.id) + "&dataset=" + eq(dataset()),
                    { "version": current }.to_json(),
                );
            };
        },
        VerifyOutcome.Inconclusive => null,
    };
    outcome
}

function issue_shipped_text(issue: Issue, toolchain: string, reported: string[] = []) -> string {
    "FIXED: *"
        + issue.title
        + "*\n"
        + issue_link(issue)
        + " · "
        + fails_fixed_text(reported, issue.version, toolchain)
        + "\nevery repro passes; reporters see it as fixed in that version (`baml toolchain use nightly`)."
}

/// Re-run every verifiable issue whose recorded toolchain is not the
/// runner's. Store-only: without it there are no issues to re-check.
function verify_issues() -> VerifyReport {
    if (!db_configured()) {
        return VerifyReport {
            toolchain: "not checked",
            checked: 0,
            shipped: [],
            skipped: ["store unconfigured"],
        };
    };
    let nightly = latest_nightly_cli();
    let current = nightly.version;
    let report = VerifyReport { toolchain: current, checked: 0, shipped: [], skipped: [] };
    if (!db_configured()) {
        report.skipped.push("store: FEEDBACK_SUPABASE_URL/KEY unset; nothing to verify");
        return report;
    };
    for (let state in verifiable_states()) {
        for (let issue in load_issues_in(state)) {
            match (needs_verification(issue, current)) {
                let why: string => report.skipped.push(issue.id + ": " + why),
                null => {
                    report.checked += 1;
                    match (verify_issue(issue, toolchain = current, cli = nightly.path)) {
                        VerifyOutcome.Fixed => report.shipped.push(issue.id),
                        VerifyOutcome.StillBroken => {
                            report.skipped.push(issue.id + ": still broken on " + current)
                        },
                        VerifyOutcome.Inconclusive => {
                            report.skipped.push(issue.id + ": inconclusive on " + current)
                        },
                    };
                },
            };
        }
    }
    log.info(
        {
            "toolchain": current,
            "verified": report.checked,
            "shipped": report.shipped.length(),
            "skipped": report.skipped.length(),
        },
    );
    report
}
