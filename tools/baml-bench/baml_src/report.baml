// Stage (h) — deterministic aggregation + rendering. Pure functions of the
// finding list; all iteration orders are pre-sorted upstream.

function build_report(
    target: string,
    blocks: CodeBlock[],
    iset: InteractionSet,
    cards: PrincipleCard[],
    findings: Finding[],
    omissions: OmissionOut[],
    routed_subjects: int,
    subjects_total: int,
) -> Report {
    let scores = compute_scores(findings);
    let omission_score = if (cards.length() > 0) {
        1.0 - ((omissions.length()) * 1.0) / ((cards.length()) * 1.0)
    } else {
        1.0
    };

    // per-card stats
    let per_card: CardStat[] = [];
    for (let c in cards) {
        let n = 0;
        let sum = 0;
        let worst = 11;
        let worst_id = "";
        for (let f in findings) {
            if (f.card_id == c.id) {
                n = n + 1;
                sum = sum + f.grade;
                if (f.grade < worst) {
                    worst = f.grade;
                    worst_id = f.block_id;
                }
            }
        }
        if (n > 0) {
            per_card
                .push(
                    CardStat {
                        card_id: c.id,
                        title: c.title,
                        n: n,
                        mean_grade: ((sum) * 1.0) / ((n) * 1.0),
                        worst_block_id: worst_id,
                        worst_grade: worst,
                    },
                );
        }
    }

    // per-file stats (block findings only; interactions have no single file)
    let file_of: map<string, string> = {};
    let files: string[] = [];
    let seen_files: map<string, bool> = {};
    for (let b in blocks) {
        file_of.set(b.id, b.file);
        if (b.file != "" && !seen_files.has(b.file)) {
            files.push(b.file);
            seen_files.set(b.file, true);
        }
    }
    let per_file: FileStat[] = [];
    for (let file in files) {
        let n = 0;
        let sum = 0;
        for (let f in findings) {
            if (!f.is_interaction) {
                let ff = match (file_of.get(f.block_id)) {
                    let v: string => v,
                    null => "",
                };
                if (ff == file) {
                    n = n + 1;
                    sum = sum + f.grade;
                }
            }
        }
        if (n > 0) {
            per_file.push(FileStat { file: file, n: n, mean_grade: ((sum) * 1.0) / ((n) * 1.0) });
        }
    }

    // per-kind stats (blocks and interactions together, keyed by subject kind)
    let kinds: string[] = [];
    let seen_kinds: map<string, bool> = {};
    for (let f in findings) {
        if (!seen_kinds.has(f.block_kind)) {
            kinds.push(f.block_kind);
            seen_kinds.set(f.block_kind, true);
        }
    }
    let per_kind: KindStat[] = [];
    for (
        let kind in kinds
            .sort_by_key(
                (k: string) -> string {
                    k
                },
            )
    ) {
        let n = 0;
        let sum = 0;
        for (let f in findings) {
            if (f.block_kind == kind) {
                n = n + 1;
                sum = sum + f.grade;
            }
        }
        if (n > 0) {
            per_kind.push(KindStat { kind: kind, n: n, mean_grade: ((sum) * 1.0) / ((n) * 1.0) });
        }
    }

    return Report {
        target: target,
        prompt_version: prompt_version(),
        block_count: blocks.length(),
        interaction_count: iset.interactions.length(),
        pair_cap_hits: iset.cap_hits,
        card_count: cards.length(),
        graded_pairs: findings.length(),
        coverage: if (subjects_total > 0) {
            ((routed_subjects) * 1.0) / ((subjects_total) * 1.0)
        } else {
            0.0
        },
        adherence_score: scores.adherence,
        commission_score: scores.commission,
        omission_score: omission_score,
        findings: findings,
        omissions: omissions,
        per_card: per_card,
        per_file: per_file,
        per_kind: per_kind,
    };
}

// Mean-grade outline of the tree down to declarations — the "AST view" of
// where quality lives.
function tree_outline(blocks: CodeBlock[], findings: Finding[]) -> string {
    let n_of: map<string, int> = {};
    let sum_of: map<string, int> = {};
    for (let f in findings) {
        if (!f.is_interaction) {
            let n = match (n_of.get(f.block_id)) {
                let v: int => v,
                null => 0,
            };
            let s = match (sum_of.get(f.block_id)) {
                let v: int => v,
                null => 0,
            };
            n_of.set(f.block_id, n + 1);
            sum_of.set(f.block_id, s + f.grade);
        }
    }
    let out = "";
    for (let b in blocks) {
        if (b.depth <= 2) {
            let indent = if (b.depth == 0) {
                ""
            } else {
                if (b.depth == 1) {
                    "  "
                } else {
                    "    "
                }
            };
            let grade_txt = match (n_of.get(b.id)) {
                let n: int => {
                    let s = match (sum_of.get(b.id)) {
                        let v: int => v,
                        null => 0,
                    };
                    " — " + f2(((s) * 1.0) / ((n) * 1.0)) + " over " + i2s(n)
                },
                null => "",
            };
            out = out + indent + b.kind + " " + b.id + grade_txt + "\n";
        }
    }
    out
}

function render_markdown(r: Report, blocks: CodeBlock[]) -> string {
    let md = "# intentionbench report\n\n";
    md = md + "target: `" + r.target + "` · prompts: " + r.prompt_version + "\n\n";
    md = md + "| metric | value |\n|---|---|\n";
    md = md + "| adherence score (1-10, weighted) | **" + f2(r.adherence_score) + "** |\n";
    md
        = md
            + "| commission score (where the design has an opinion) | "
            + f2(r.commission_score)
            + " |\n";
    md = md + "| omission score (0-1) | " + f2(r.omission_score) + " |\n";
    md
        = md
            + "| blocks / interactions | "
            + i2s(r.block_count)
            + " / "
            + i2s(r.interaction_count)
            + " (cap hits "
            + i2s(r.pair_cap_hits)
            + ") |\n";
    md = md + "| graded (subject, card) pairs | " + i2s(r.graded_pairs) + " |\n";
    md = md + "| subject coverage | " + f2(r.coverage * 100.0) + "% |\n";
    md = md + "| gradeable cards | " + i2s(r.card_count) + " |\n";

    // slop report: everything that survived refutation with a misuse verdict
    md = md + "\n## Slop report\n\n";
    let slop = r
        .findings
        .filter(
            (f: Finding) -> bool {
                (f.verdict == "fighting" || f.verdict == "reinventing") && !f.refuted
            },
        )
        .sort_by_key(
            (f: Finding) -> string {
                pad6(f.grade) + f.block_id + f.card_id
            },
        );
    if (slop.length() == 0) {
        md = md + "none found.\n";
    }
    for (let f in slop) {
        md
            = md
                + "### "
                + f.block_id
                + " ("
                + f.block_kind
                + ") × "
                + f.card_id
                + " — grade "
                + i2s(f.grade)
                + " ("
                + f.verdict
                + ")\n\n";
        md = md + "evidence:\n\n```\n" + f.evidence + "\n```\n\n";
        match (f.suggested_form) {
            let s: string => {
                md = md + "intended form:\n\n```\n" + s + "\n```\n\n";
            },
            null => null,
        }
    }

    // omissions
    md = md + "\n## Omissions\n\n";
    if (r.omissions.length() == 0) {
        md = md + "none found.\n";
    }
    for (let o in r.omissions) {
        md
            = md
                + "- **"
                + o.card_id
                + "** — "
                + o.description
                + " (evidence: "
                + o.evidence
                + ")\n";
    }

    // per-card table
    md = md + "\n## Per-principle\n\n| card | n | mean | worst |\n|---|---|---|---|\n";
    for (
        let c in r
            .per_card
            .sort_by_key(
                (c: CardStat) -> string {
                    f2(c.mean_grade) + c.card_id
                },
            )
    ) {
        md
            = md
                + "| "
                + c.card_id
                + " "
                + c.title
                + " | "
                + i2s(c.n)
                + " | "
                + f2(c.mean_grade)
                + " | "
                + i2s(c.worst_grade)
                + " @ "
                + c.worst_block_id
                + " |\n";
    }

    // per-kind table
    md = md + "\n## Per-kind\n\n| kind | n | mean |\n|---|---|---|\n";
    for (let k in r.per_kind) {
        md = md + "| " + k.kind + " | " + i2s(k.n) + " | " + f2(k.mean_grade) + " |\n";
    }

    // per-file table
    md = md + "\n## Per-file\n\n| file | n | mean |\n|---|---|---|\n";
    for (
        let fl in r
            .per_file
            .sort_by_key(
                (fl: FileStat) -> string {
                    f2(fl.mean_grade) + fl.file
                },
            )
    ) {
        md = md + "| " + fl.file + " | " + i2s(fl.n) + " | " + f2(fl.mean_grade) + " |\n";
    }

    // tree outline
    md = md + "\n## Tree\n\n```\n" + tree_outline(blocks, r.findings) + "```\n";

    // dropped / refuted appendix
    let refuted = r
        .findings
        .filter(
            (f: Finding) -> bool {
                f.refuted
            },
        );
    md = md + "\n## Refuted findings (" + i2s(refuted.length()) + ")\n\n";
    for (let f in refuted) {
        let reason = match (f.refute_reason) {
            let s: string => s,
            null => "",
        };
        md
            = md
                + "- "
                + f.block_id
                + " × "
                + f.card_id
                + " (raw grade "
                + i2s(f.raw_grade)
                + "): "
                + reason
                + "\n";
    }
    let unverified = r
        .findings
        .filter(
            (f: Finding) -> bool {
                !f.evidence_verified
            },
        );
    md
        = md
            + "\n## Evidence-unverified (neutralized) findings ("
            + i2s(unverified.length())
            + ")\n\n";
    for (let f in unverified) {
        md = md + "- " + f.block_id + " × " + f.card_id + " (raw grade " + i2s(f.raw_grade) + ")\n";
    }
    md
}
