// Stage (h) — deterministic aggregation + rendering. Pure functions of the
// finding list; all iteration orders are pre-sorted upstream.

function build_report(
    target: string,
    chunks: Chunk[],
    edges: Edge[],
    cards: PrincipleCard[],
    findings: Finding[],
    omissions: OmissionOut[],
    routed_chunks: int,
) -> Report {
    // weighted mean over effective grades
    let wsum = 0.0;
    let gsum = 0.0;
    for (let f in findings) {
        wsum = wsum + f.weight;
        gsum = gsum + f.weight * ((f.grade) * 1.0);
    }
    let adherence = if (wsum > 0.0) {
        gsum / wsum
    } else {
        0.0
    };

    // commission = mean over pairs graded <= 6 candidates? No: mean over pairs
    // whose verdict is a misuse call (fighting/reinventing) plus adherent pairs,
    // i.e. everything non-neutral — how well the code does where the design has
    // an opinion.
    let cw = 0.0;
    let cg = 0.0;
    for (let f in findings) {
        if (f.verdict != "neutral") {
            cw = cw + f.weight;
            cg = cg + f.weight * ((f.grade) * 1.0);
        }
    }
    let commission = if (cw > 0.0) {
        cg / cw
    } else {
        adherence
    };

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
                    worst_id = f.chunk_id;
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
                        worst_chunk_id: worst_id,
                        worst_grade: worst,
                    },
                );
        }
    }

    // per-file stats
    let file_of: map<string, string> = {};
    for (let ch in chunks) {
        file_of.set(ch.id, ch.file);
    }
    let files: string[] = [];
    let seen_files: map<string, bool> = {};
    for (let ch in chunks) {
        if (!seen_files.has(ch.file)) {
            files.push(ch.file);
            seen_files.set(ch.file, true);
        }
    }
    let per_file: FileStat[] = [];
    for (let file in files) {
        let n = 0;
        let sum = 0;
        for (let f in findings) {
            let ff = match (file_of.get(f.chunk_id)) {
                let v: string => v,
                null => "",
            };
            if (ff == file) {
                n = n + 1;
                sum = sum + f.grade;
            }
        }
        if (n > 0) {
            per_file.push(FileStat { file: file, n: n, mean_grade: ((sum) * 1.0) / ((n) * 1.0) });
        }
    }

    return Report {
        target: target,
        prompt_version: prompt_version(),
        chunk_count: chunks.length(),
        edge_count: edges.length(),
        card_count: cards.length(),
        graded_pairs: findings.length(),
        coverage: if (chunks.length() > 0) {
            ((routed_chunks) * 1.0) / ((chunks.length()) * 1.0)
        } else {
            0.0
        },
        adherence_score: adherence,
        commission_score: commission,
        omission_score: omission_score,
        findings: findings,
        omissions: omissions,
        per_card: per_card,
        per_file: per_file,
    };
}

function render_markdown(r: Report) -> string {
    let md = "# BAML Adherence Report\n\n";
    md = md + "target: `" + r.target + "` · prompts: " + r.prompt_version + "\n\n";
    md = md + "| metric | value |\n|---|---|\n";
    md = md + "| adherence score (1-10, weighted) | **" + f2(r.adherence_score) + "** |\n";
    md
        = md
            + "| commission score (where the design has an opinion) | "
            + f2(r.commission_score)
            + " |\n";
    md = md + "| omission score (0-1) | " + f2(r.omission_score) + " |\n";
    md = md + "| chunks / edges | " + i2s(r.chunk_count) + " / " + i2s(r.edge_count) + " |\n";
    md = md + "| graded (chunk, card) pairs | " + i2s(r.graded_pairs) + " |\n";
    md = md + "| chunk coverage | " + f2(r.coverage * 100.0) + "% |\n";
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
                pad6(f.grade) + f.chunk_id + f.card_id
            },
        );
    if (slop.length() == 0) {
        md = md + "none found.\n";
    }
    for (let f in slop) {
        md
            = md
                + "### "
                + f.chunk_id
                + " × "
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
                + c.worst_chunk_id
                + " |\n";
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
                + f.chunk_id
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
        md = md + "- " + f.chunk_id + " × " + f.card_id + " (raw grade " + i2s(f.raw_grade) + ")\n";
    }
    md
}
