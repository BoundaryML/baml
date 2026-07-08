// Stage (b) — interaction blocks, all deterministic.
//
// Three kinds:
//   by_depth  — containment: one block per (child, parent) edge, for every
//               block with a parent. Never capped.
//   reference — static identifier references between top-level decls
//               (calls / uses_type / exercises), the v1 edge scan.
//   by_level  — sibling pairs under a common parent, canonical a_id < b_id,
//               capped per parent (default 30, BENCH_PAIR_CAP override) with
//               statically-evidenced pairs ranked first.
//
// Reference blocks win dedupe against by_level blocks over the same pair (they
// carry the richer edge_kind).

function pair_cap() -> int {
    let raw = baml.env.get("BENCH_PAIR_CAP") catch (_) {
        _ => null
    };
    let n = match (raw) {
        let v: string => {
            baml.json.from_string<int>(v) catch (_) {
                _ => 30
            }
        },
        null => 30,
    };
    n.clamp(1, 500)
}

// ---- by_depth ------------------------------------------------------------------

function depth_interactions(blocks: CodeBlock[]) -> InteractionBlock[] {
    let out: InteractionBlock[] = [];
    for (let b in blocks) {
        match (b.parent_id) {
            let p: string => {
                out.push(
                    InteractionBlock {
                        id: "I:d:" + b.id + "^" + p,
                        kind: "by_depth",
                        a_id: b.id,
                        b_id: p,
                        parent_id: p,
                        edge_kind: "contains",
                        evidenced: true,
                    },
                );
            },
            null => null,
        }
    }
    out
}

// ---- reference (v1 identifier scan over top-level decls) -------------------------

// Decl A references decl B when B's declared name appears as an identifier in
// A's source. test -> anything: "exercises"; -> type-ish decl: "uses_type";
// otherwise "calls". Same-file pairs group under the file, cross-file under
// PROGRAM.
function reference_interactions(blocks: CodeBlock[]) -> InteractionBlock[] {
    let decls: CodeBlock[] = blocks
        .filter(
            (b: CodeBlock) -> bool {
                b.depth == 2
            },
        );
    let by_name: map<string, string[]> = {};
    let kind_of: map<string, string> = {};
    let file_of: map<string, string> = {};
    for (let c in decls) {
        let ids: string[] = match (by_name.get(c.name)) {
            let v: string[] => v,
            null => {
                let empty: string[] = [];
                empty
            },
        };
        ids.push(c.id);
        by_name.set(c.name, ids);
        kind_of.set(c.id, c.kind);
        file_of.set(c.id, c.file);
    }
    let out: InteractionBlock[] = [];
    for (let c in decls) {
        for (let ident in identifiers(c.source)) {
            if (ident == c.name) {
                // self-reference; skip
            } else {
                match (by_name.get(ident)) {
                    let targets: string[] => {
                        for (let to in targets) {
                            let to_kind = match (kind_of.get(to)) {
                                let k: string => k,
                                null => "",
                            };
                            let kind = if (c.kind == "test") {
                                "exercises"
                            } else {
                                if (
                                    to_kind == "class"
                                        || to_kind == "enum"
                                        || to_kind == "interface"
                                        || to_kind == "type_alias"
                                ) {
                                    "uses_type"
                                } else {
                                    "calls"
                                }
                            };
                            let to_file = match (file_of.get(to)) {
                                let f: string => f,
                                null => "",
                            };
                            let parent = if (to_file == c.file) {
                                "F:" + c.file
                            } else {
                                "PROGRAM"
                            };
                            out.push(
                                InteractionBlock {
                                    id: "I:r:" + c.id + "->" + to,
                                    kind: "reference",
                                    a_id: c.id,
                                    b_id: to,
                                    parent_id: parent,
                                    edge_kind: kind,
                                    evidenced: true,
                                },
                            );
                        }
                    },
                    null => null,
                }
            }
        }
    }
    out
}

// ---- by_level -------------------------------------------------------------------

// All unordered sibling pairs per parent, capped. Rank: statically-evidenced
// pairs first (one sibling's name appears in the other's source), then the
// rest; both tiers ordered lexicographically by pair id. Pairs already covered
// by a reference block (either direction) are skipped.
function level_interactions(
    blocks: CodeBlock[],
    refs: InteractionBlock[],
    cap: int,
) -> InteractionBlock[] {
    let ref_pairs: map<string, bool> = {};
    for (let r in refs) {
        ref_pairs.set(r.a_id + "~" + r.b_id, true);
        ref_pairs.set(r.b_id + "~" + r.a_id, true);
    }
    // Precompute each sibling's name and identifier set ONCE — the pair loop is
    // O(n²) per parent and a per-pair identifiers() scan is unaffordably slow.
    let by_id = block_index(blocks);
    let name_of: map<string, string> = {};
    let idents_of: map<string, map<string, bool>> = {};
    for (let parent in blocks) {
        if (parent.child_ids.length() >= 2) {
            for (let cid in parent.child_ids) {
                if (idents_of.has(cid)) {
                    // already computed
                } else {
                    match (by_id.get(cid)) {
                        let b: CodeBlock => {
                            name_of.set(cid, b.name);
                            let set: map<string, bool> = {};
                            for (let ident in identifiers(b.source)) {
                                set.set(ident, true);
                            }
                            idents_of.set(cid, set);
                        },
                        null => null,
                    }
                }
            }
        }
    }
    let out: InteractionBlock[] = [];
    for (let parent in blocks) {
        let n = parent.child_ids.length();
        if (n >= 2) {
            // rank keys: "<0|1 evidenced-first>\t<pair key>"
            let keys: string[] = [];
            let i = 0;
            while (i < n) {
                let j = i + 1;
                while (j < n) {
                    let a = match (parent.child_ids.at(i)) {
                        let v: string => v,
                        null => "",
                    };
                    let b = match (parent.child_ids.at(j)) {
                        let v: string => v,
                        null => "",
                    };
                    // canonical order
                    let lo = if (a < b) {
                        a
                    } else {
                        b
                    };
                    let hi = if (a < b) {
                        b
                    } else {
                        a
                    };
                    if (ref_pairs.has(lo + "~" + hi)) {
                        // reference block already covers this pair
                    } else {
                        let ev = pair_evidenced(lo, hi, name_of, idents_of);
                        let tier = if (ev) {
                            "0"
                        } else {
                            "1"
                        };
                        keys.push(tier + "\t" + lo + "~" + hi);
                    }
                    j = j + 1;
                }
                i = i + 1;
            }
            let ranked = keys
                .sort_by_key(
                    (k: string) -> string {
                        k
                    },
                );
            let taken = 0;
            for (let k in ranked) {
                if (taken < cap) {
                    let sep = k.index_of("\t");
                    let pair = k.substring(sep + 1, k.length());
                    let tilde = pair.index_of("~");
                    let lo = pair.substring(0, tilde);
                    let hi = pair.substring(tilde + 1, pair.length());
                    out.push(
                        InteractionBlock {
                            id: "I:l:" + lo + "~" + hi,
                            kind: "by_level",
                            a_id: lo,
                            b_id: hi,
                            parent_id: parent.id,
                            edge_kind: "sibling",
                            evidenced: k.starts_with("0"),
                        },
                    );
                    taken = taken + 1;
                }
            }
        }
    }
    out
}

// One sibling's declared name appears as an identifier in the other's source
// (precomputed identifier sets — this runs once per sibling pair).
function pair_evidenced(
    a_id: string,
    b_id: string,
    name_of: map<string, string>,
    idents_of: map<string, map<string, bool>>,
) -> bool {
    let a_name = match (name_of.get(a_id)) {
        let v: string => v,
        null => "",
    };
    let b_name = match (name_of.get(b_id)) {
        let v: string => v,
        null => "",
    };
    let a_has_b = match (idents_of.get(a_id)) {
        let set: map<string, bool> => b_name != "" && set.has(b_name),
        null => false,
    };
    if (a_has_b) {
        return true;
    }
    match (idents_of.get(b_id)) {
        let set: map<string, bool> => a_name != "" && set.has(a_name),
        null => false,
    }
}

// ---- assembly --------------------------------------------------------------------

class InteractionSet {
    interactions: InteractionBlock[],
    cap_hits: int, // parents whose by_level pairs were truncated
}

// All interaction blocks for a tree, sorted by id.
function build_interactions(blocks: CodeBlock[]) -> InteractionSet {
    let cap = pair_cap();
    let out: InteractionBlock[] = [];
    for (let d in depth_interactions(blocks)) {
        out.push(d);
    }
    let refs = reference_interactions(blocks);
    // dedupe duplicate reference ids (same pair found via multiple identifiers
    // is impossible — identifiers() dedupes — but A->B and B->A both surviving
    // is fine; they are distinct directed references)
    for (let r in refs) {
        out.push(r);
    }
    // cap-hit count: parents with more eligible pairs than the cap
    let cap_hits = 0;
    for (let parent in blocks) {
        let n = parent.child_ids.length();
        if ((n * (n - 1) / 2) > cap) {
            cap_hits = cap_hits + 1;
        }
    }
    for (let l in level_interactions(blocks, refs, cap)) {
        out.push(l);
    }
    return InteractionSet {
        interactions: out
            .sort_by_key(
                (ib: InteractionBlock) -> string {
                    ib.id
                },
            ),
        cap_hits: cap_hits,
    };
}

// The "source" a grader (and the evidence gate) sees for an interaction: both
// endpoints under headers, big endpoints truncated.
function interaction_source(ib: InteractionBlock, by_id: map<string, CodeBlock>) -> string {
    let a = match (by_id.get(ib.a_id)) {
        let v: CodeBlock => v,
        null => {
            return "";
        },
    };
    let b = match (by_id.get(ib.b_id)) {
        let v: CodeBlock => v,
        null => {
            return "";
        },
    };
    "# block A: "
        + a.id
        + " ("
        + a.kind
        + ")\n"
        + truncate(a.source, 3000)
        + "\n# block B: "
        + b.id
        + " ("
        + b.kind
        + ")\n"
        + truncate(b.source, 3000)
        + "\n"
}

// Up to `limit` one-line descriptions of a decl's reference neighborhood, for
// intention/grading context. Deterministic: interactions are pre-sorted.
function neighbor_context(
    block_id: string,
    interactions: InteractionBlock[],
    by_id: map<string, CodeBlock>,
    limit: int,
) -> string {
    let out = "";
    let n = 0;
    for (let e in interactions) {
        if (e.kind == "reference" && (n < limit) && (e.a_id == block_id || e.b_id == block_id)) {
            let other = if (e.a_id == block_id) {
                e.b_id
            } else {
                e.a_id
            };
            let dir = if (e.a_id == block_id) {
                "-> "
            } else {
                "<- "
            };
            let desc = match (by_id.get(other)) {
                let ob: CodeBlock => ob.kind + " | " + block_head(ob),
                null => "",
            };
            out = out + dir + e.edge_kind + " " + other + " (" + desc + ")\n";
            n = n + 1;
        }
    }
    out
}
