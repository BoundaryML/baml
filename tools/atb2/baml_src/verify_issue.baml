// Re-confirm open issues on the toolchain this runner was built with: the
// latest published nightly (deploy/build-cli.sh). A fix that lands on
// canary reaches the next nightly, the runner rebuilds on it, and the next
// pass finds the repros passing: the issue ships, the reporters' `baml
// feedback status` says which nightly fixed it, and no agent run is wasted.
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
    null
}

/// Today, as the Shipped status records it.
function today() -> string {
    baml.time.ZonedDateTime.now_in("UTC").to_string().slice(0, 10)
}

/// Re-run one issue's repros on the runner's toolchain. Fixed: the issue
/// becomes Shipped on that version, `fixed_in` is set for the reporters'
/// CLI, and the thread hears. StillBroken: the issue's version advances so
/// the next pass skips it. Inconclusive: nothing changes.
function verify_issue(issue: Issue, toolchain: string? = null) -> VerifyOutcome {
    let current = toolchain ?? toolchain_version();
    //# run every repro out of process on the current toolchain
    let verdicts: Verdict[] = [];
    let i = 0;
    for (let r in issue.repros) {
        i += 1;
        verdicts.push(check_on_canary(r, i, cli = check_cli()));
    }
    let outcome = fold_verdicts(verdicts);
    log.info({ "issue": issue.id, "verify": outcome.to_string(), "toolchain": current });
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
    let current = toolchain_version();
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
                    match (verify_issue(issue, toolchain = current)) {
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

testset "verify_issue" {
    test "fixed only when every repro passes; a failure outranks the unknown" {
        assert.equal(fold_verdicts([Verdict.Fixed, Verdict.Fixed]), VerifyOutcome.Fixed);
        assert.equal(
            fold_verdicts([Verdict.Fixed, Verdict.Inconclusive]),
            VerifyOutcome.Inconclusive,
        );
        assert.equal(
            fold_verdicts([Verdict.Inconclusive, Verdict.StillBroken]),
            VerifyOutcome.StillBroken,
        );
        assert.equal(fold_verdicts([Verdict.Fixed, Verdict.StillBroken]), VerifyOutcome.StillBroken);
        assert.equal(fold_verdicts([]), VerifyOutcome.Inconclusive);
    }
    test "an issue is re-run only on a toolchain it has not been seen broken on" {
        let issue = gh_issue(7, Difficulty.Easy);
        let seen = baml.json.from_json<map<string, json>>(issue_row(issue));
        let repro = Repro {
            files: { "main.baml": "function main() -> int { 1 }" },
            command: "baml check",
            setup: null,
            expectation: ShouldCompile { check: "should_compile" },
        };
        seen.set("repros", [repro].to_json());
        seen.set("version", "0.18.1-nightly.20260908.a".to_json());
        let with_repro = issue_from_row(seen.to_json());
        assert.equal(
            needs_verification(with_repro, "0.18.1-nightly.20260908.a") ?? "",
            "already confirmed broken on 0.18.1-nightly.20260908.a",
        );
        assert.equal(needs_verification(with_repro, "0.18.1-nightly.20260909.a") == null, true);
        assert.equal(needs_verification(with_repro, "unknown") ?? "", "runner toolchain unknown");
        let none: Repro[] = [];
        seen.set("repros", none.to_json());
        assert.equal(
            needs_verification(issue_from_row(seen.to_json()), "0.18.1-nightly.20260909.a") ?? "",
            "no repros to run",
        );
    }
    test "the ship message says what fails and what fixes" {
        let issue = gh_issue(7, Difficulty.Easy);
        let text = issue_shipped_text(issue, "0.18.1-nightly.20260908.a", reported = ["0.17.1", "0.18.0"]);
        assert.equal(
            text.index_of("fails on 0.17.1, 0.18.0 · fixed on 0.18.1-nightly.20260908.a") != null,
            true,
        );
        assert.equal(fails_fixed_text([], "0.18.0", "0.18.1"), "fails on 0.18.0 · fixed on 0.18.1");
    }
    test "today is a date" {
        assert.equal(today().length(), 10);
    }
}
