// Offline smoke harness. SeedSmoke mirrors run_pipeline's cache-key derivation
// exactly and pre-seeds canned judgments, so a following `Run`/`Bench` replays
// end-to-end with ZERO LLM calls. Used to prove pipeline determinism (two runs
// after one seed must produce byte-identical reports) and to exercise the full
// orchestration (analysis -> grading -> evidence gate -> refutation -> report)
// offline.
//
//   baml run -f SeedSmoke -- SeedSmoke --target <dir> --out <dir> --catalog <md>
//   baml run -f Run       -- Run       --target <dir> --out <dir> --catalog <md>

function SeedSmoke(target: string, out: string, catalog: string) -> int {
    let catalog_text = baml.fs.read(default_catalog(catalog)) catch (_) {
        _ => ""
    };
    if (catalog_text == "") {
        baml.io.eprintln("cannot read catalog");
        return 1;
    }
    let cards = gradeable_cards(parse_cards(catalog_text));
    let chunks = chunk_project(target)
        .filter(
            (c: Chunk) -> bool {
                c.kind != "config"
            },
        );
    let edges = build_edges(chunks);
    let cache_dir = out + "/cache";
    let index = card_index(cards);

    let first_card = match (cards.at(0)) {
        let c: PrincipleCard => c,
        null => {
            PrincipleCard {
                id: "",
                title: "",
                status: "",
                weight: 0.0,
                trigger: "",
                detectability: "",
                body: "",
            }
        },
    };

    let card_by_id: map<string, PrincipleCard> = {};
    for (let card in cards) {
        card_by_id.set(card.id, card);
    }
    let intent_table = "";
    for (let c in chunks) {
        let neighbors = neighbor_context(c.id, edges, chunks, 8);
        // mirror the prefilter: seed the raw LLM answer as "all candidates",
        // and compute the final routed set the way cached_analyze will.
        let pre = prefilter_for(c, cards);
        let raw_ids: string[] = [];
        for (let cand in pre.candidates) {
            raw_ids.push(cand.id);
        }
        let final_ids = merge_routed(pre, raw_ids);
        let analysis = ChunkAnalysis {
            goal: "smoke goal for " + c.id,
            mechanism: "smoke mechanism",
            confidence: 3,
            card_ids: raw_ids,
        };
        let candidate_index = card_index(pre.candidates);
        let static_key = pre.static_ids.join(",");
        let akey = cache_key(
            "analyze@" + fast_model(),
            c.id + "\n" + c.source + "\n" + neighbors + "\n" + static_key + "\n" + candidate_index,
        );
        store_json(cache_dir, akey, analysis.to_json());
        intent_table
            = intent_table
                + c.id
                + " | "
                + c.kind
                + " | "
                + analysis.goal
                + " | "
                + analysis.mechanism
                + "\n";

        // (e) multi-card grade: canned low grade with the chunk's first nonempty
        // line as evidence, so the deterministic evidence gate passes.
        let evidence = "";
        for (let line in c.source.lines()) {
            if (evidence == "" && line.trim() != "") {
                evidence = line.trim();
            }
        }
        let grades: CardGrade[] = [];
        let ids_key = "";
        let bodies = "";
        for (let id in final_ids) {
            match (card_by_id.get(id)) {
                let card: PrincipleCard => {
                    ids_key = ids_key + card.id + ",";
                    bodies = bodies + card.body + "\n---\n";
                    grades
                        .push(
                            CardGrade {
                                card_id: card.id,
                                grade: 3,
                                verdict: "fighting",
                                evidence: evidence,
                                suggested_form: "smoke suggested form",
                            },
                        );
                    // (f) refutation of each low grade: finding stands
                    let fkey = cache_key(
                        "refute@" + judge_model(),
                        card.id + "\n" + c.id + "\n" + c.source + "\n" + i2s(3) + "\n" + evidence,
                    );
                    store_json(cache_dir, fkey, RefuteOut { refuted: false, reason: "" }.to_json());
                },
                null => null,
            }
        }
        if (grades.length() > 0) {
            let gkey = cache_key("grade@" + judge_model(), ids_key + "\n" + bodies + "\n" + c.id + "\n" + c.source);
            store_json(cache_dir, gkey, MultiGradeOut { grades: grades }.to_json());
        }
    }

    // (g) omissions: none
    let okey = cache_key("omissions@" + judge_model(), intent_table + "\n" + index);
    let none: OmissionOut[] = [];
    store_json(cache_dir, okey, OmissionScanOut { omissions: none }.to_json());

    baml.io.println("seeded " + i2s(chunks.length()) + " chunks against card " + first_card.id);
    0
}
