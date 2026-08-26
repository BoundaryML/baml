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
// Rerun the ranking when the team shifts; the table below is the only
// thing that changes.

/// GitHub login of the engineer who owns a subsystem.
function shepherd_for(subsystem: Subsystem) -> string {
    match (subsystem) {
        Subsystem.Syntax => "aaronvg",
        Subsystem.Compiler => "codeshaunted",
        Subsystem.Runtime => "antoniosarosi",
        Subsystem.StdLibrary => "2kai2kai2",
        Subsystem.Tooling => "sxlijin",
    }
}

/// Route an issue to the shepherd of its subsystem. Idempotent: an issue
/// that already has a shepherd keeps it.
function organize_issue(issue: Issue) -> Issue {
    if (issue.shepherd != null) {
        return issue;
    };
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
    }
}

testset "organize_issue" {
    let unassigned = Issue {
        id: "ISSUE-1",
        title: "rust codegen panics on naming_convention",
        description: "…",
        shepherd: null,
        subsystem: Subsystem.Tooling,
        repros: [],
        version: "0.17.0",
        feedback_ids: ["FB-1"],
        status: Open { state: "open" },
        comments: [],
        resolution_plan: null,
    };

    test "routes by subsystem" {
        assert.equal(organize_issue(unassigned).shepherd ?? "", "sxlijin");
    }

    test "every subsystem has an owner" {
        for (
            let s in [
                Subsystem.Syntax,
                Subsystem.Compiler,
                Subsystem.Runtime,
                Subsystem.StdLibrary,
                Subsystem.Tooling,
            ]
        ) {
            assert.is_true(shepherd_for(s).length() > 0);
        }
    }

    test "keeps an existing shepherd" {
        let assigned = organize_issue(unassigned);
        let again = organize_issue(
            Issue {
                id: assigned.id,
                title: assigned.title,
                description: assigned.description,
                shepherd: "someone-else",
                subsystem: Subsystem.Compiler,
                repros: assigned.repros,
                version: assigned.version,
                feedback_ids: assigned.feedback_ids,
                status: assigned.status,
                comments: assigned.comments,
                resolution_plan: assigned.resolution_plan,
            },
        );
        assert.equal(again.shepherd ?? "", "someone-else");
    }

    test "nothing but the shepherd changes" {
        let out = organize_issue(unassigned);
        assert.equal(out.id, unassigned.id);
        assert.equal(out.subsystem, unassigned.subsystem);
        assert.equal(out.feedback_ids.length(), 1);
    }
}
