// issue -> shepherd. Deterministic: each subsystem has one owner, and an
// issue goes to the owner of its subsystem.
//
// Ownership was derived from BoundaryML/baml's last 400 merged PRs into
// canary (2026-07-09 .. 2026-08-26): every changed file was mapped to a
// subsystem by path (parser/lexer/syntax/fmt -> Syntax; compiler2_*,
// diagnostics, types -> Compiler; bex_*, exec, sys_* -> Runtime;
// baml_builtins2 -> StdLibrary; cli, lsp, project, sdks, bridges, typescript2 ->
// Tooling) and the author with the most PRs touching a subsystem owns it.
//
//   Syntax      aaronvg 11 PRs, codeshaunted 8, 2kai2kai2 4
//   Compiler    codeshaunted 38, antoniosarosi 29, 2kai2kai2 26
//   Runtime     antoniosarosi 23, 2kai2kai2 19, sxlijin 19
//   StdLibrary  2kai2kai2 20, antoniosarosi 17, codeshaunted 13
//   Tooling     sxlijin 81, codeshaunted 51, 2kai2kai2 41
//
// PR counts miss the founders, who touch everything and own nothing by
// that measure (hellovai: 21 tooling / 6 runtime / 6 compiler / 3 syntax
// PRs in the window, top-3 nowhere). The defaults are listed last on
// every subsystem, and are the only owners of Subsystem.Unknown.
//
// Rerun the ranking when the team shifts; the table below is the only
// thing that changes.

/// Where an issue goes when no subsystem owner fits: the founders. Also
/// listed last on every subsystem.
function default_shepherds() -> string[] {
    ["hellovai", "aaronvg"]
}

/// GitHub logins of the engineers who own a subsystem, most active first,
/// then the defaults.
function shepherds_for(subsystem: Subsystem) -> string[] {
    //# ranked owners per subsystem (from merged PRs)
    let owners: string[] = match (subsystem) {
        Subsystem.Syntax => ["aaronvg", "codeshaunted", "2kai2kai2"],
        Subsystem.Compiler => ["codeshaunted", "antoniosarosi", "2kai2kai2"],
        Subsystem.Runtime => ["antoniosarosi", "2kai2kai2", "sxlijin"],
        Subsystem.StdLibrary => ["2kai2kai2", "antoniosarosi", "codeshaunted"],
        Subsystem.Tooling => ["sxlijin", "codeshaunted", "2kai2kai2"],
        Subsystem.Unknown => [],
    };
    //# the founders close every list
    owners.concat(default_shepherds())
}

/// The shepherd an issue in this subsystem goes to: the most active owner.
function shepherd_for(subsystem: Subsystem) -> string {
    shepherds_for(subsystem).at(0) ?? baml.sys.panic("no shepherds for " + subsystem.to_string())
}

/// Route an issue to the shepherd of its subsystem. Idempotent: an issue
/// that already has a shepherd keeps it.
function organize_issue(issue: Issue) -> Issue {
    //# keep an existing shepherd
    if (issue.shepherd != null) {
        return issue;
    };
    //# route by subsystem to the most active owner
    let shepherd = shepherd_for(issue.subsystem);
    log.info({ "issue": issue.id, "subsystem": issue.subsystem.to_string(), "shepherd": shepherd });
    Issue {
        id: issue.id,
        title: issue.title,
        description: issue.description,
        shepherd: shepherd,
        subsystem: issue.subsystem,
        repros: issue.repros,
        version: issue.version,
        feedback_ids: issue.feedback_ids,
        status: issue.status,
        comments: issue.comments,
        resolution_plan: issue.resolution_plan,
        difficulty: issue.difficulty,
        design_doc: issue.design_doc,
    }
}

/// Test fixture: an issue in `subsystem`, optionally already shepherded.
function issue_in(subsystem: Subsystem, shepherd: string?) -> Issue {
    Issue {
        id: "ISSUE-1",
        title: "t",
        description: "…",
        shepherd: shepherd,
        subsystem: subsystem,
        repros: [],
        version: "0.17.0",
        feedback_ids: ["FB-1"],
        status: Open { state: "open" },
        comments: [],
        resolution_plan: null,
        difficulty: null,
        design_doc: null,
    }
}

// Checks the deterministic matcher: every subsystem routes to the first of
// its owners, and organize_issue only ever touches `shepherd`.
testset "organize_issue" {
    for (
        let s in [
            Subsystem.Syntax,
            Subsystem.Compiler,
            Subsystem.Runtime,
            Subsystem.StdLibrary,
            Subsystem.Tooling,
        ]
    ) {
        test ("routes " + s.to_string() + " to its first owner") {
            let owners = shepherds_for(s);
            assert.is_true(owners.length() >= 2);
            assert.equal(organize_issue(issue_in(s, null)).shepherd ?? "", owners.at(0) ?? "");
        }
    }

    test "the defaults back every subsystem" {
        for (
            let s in [
                Subsystem.Syntax,
                Subsystem.Compiler,
                Subsystem.Runtime,
                Subsystem.StdLibrary,
                Subsystem.Tooling,
            ]
        ) {
            let owners = shepherds_for(s);
            assert.equal(owners.at(owners.length() - 2) ?? "", "hellovai");
            assert.equal(owners.at(owners.length() - 1) ?? "", "aaronvg");
        }
    }

    test "no matching subsystem goes to the defaults" {
        assert.equal(organize_issue(issue_in(Subsystem.Unknown, null)).shepherd ?? "", "hellovai");
        assert.equal(shepherds_for(Subsystem.Unknown).length(), 2);
    }

    test "known routes" {
        assert.equal(organize_issue(issue_in(Subsystem.Tooling, null)).shepherd ?? "", "sxlijin");
        assert.equal(
            organize_issue(issue_in(Subsystem.Compiler, null)).shepherd ?? "",
            "codeshaunted",
        );
    }

    test "keeps an existing shepherd" {
        assert.equal(
            organize_issue(issue_in(Subsystem.Compiler, "someone-else")).shepherd ?? "",
            "someone-else",
        );
    }

    test "nothing but the shepherd changes" {
        let before = issue_in(Subsystem.Runtime, null);
        let after = organize_issue(before);
        assert.equal(after.id, before.id);
        assert.equal(after.subsystem, before.subsystem);
        assert.equal(after.feedback_ids.length(), 1);
        assert.equal(after.version, before.version);
    }
}
