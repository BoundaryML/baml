// intentionbench entry points.
//
//   Bench — the CLI. Packaged with `baml pack Bench -o intentionbench` into a
//   standalone binary that takes the BAML project to inspect, grades it
//   against the embedded design-principles catalog, writes report files, and
//   prints the report in the terminal:
//
//       intentionbench --target <project> [--out <dir>] [--catalog <md>]
//
//   Dev entrypoints (baml run -f <Fn> -- <Fn> --args…):
//       Scan  — deterministic stages only (block tree + interactions); no LLM
//       Cards — parse + list the catalog
//       Run   — Bench minus the embedded catalog default (explicit paths)

function default_catalog(catalog: string) -> string {
    if (catalog != "") {
        catalog
    } else {
        "design-principles.md"
    }
}

// Print the block tree (indented) and interaction summary. No LLM, no env.
function Scan(target: string) -> int {
    let blocks = build_tree(target);
    let by_id = block_index(blocks);
    baml.io.println("blocks: " + i2s(blocks.length()));
    print_subtree("PROGRAM", by_id, "");
    let iset = build_interactions(blocks);
    let by_depth = 0;
    let by_level = 0;
    let refs = 0;
    for (let ib in iset.interactions) {
        if (ib.kind == "by_depth") {
            by_depth = by_depth + 1;
        }
        if (ib.kind == "by_level") {
            by_level = by_level + 1;
        }
        if (ib.kind == "reference") {
            refs = refs + 1;
        }
    }
    baml.io
        .println(
            "interactions: "
                + i2s(iset.interactions.length())
                + " (by_depth "
                + i2s(by_depth)
                + ", by_level "
                + i2s(by_level)
                + ", reference "
                + i2s(refs)
                + "), cap hits: "
                + i2s(iset.cap_hits),
        );
    0
}

function print_subtree(id: string, by_id: map<string, CodeBlock>, indent: string) -> null {
    match (by_id.get(id)) {
        let b: CodeBlock => {
            let span = if (b.depth == 0) {
                ""
            } else {
                " (" + i2s(b.start_line) + "-" + i2s(b.end_line) + ")"
            };
            baml.io.println(indent + b.kind + "\t" + b.id + span);
            for (let c in b.child_ids) {
                print_subtree(c, by_id, indent + "  ");
            }
        },
        null => null,
    }
    null
}

function Cards(catalog: string) -> int {
    let text = baml.fs.read(default_catalog(catalog)) catch (_) {
        _ => ""
    };
    if (text == "") {
        baml.io.eprintln("cannot read catalog: " + default_catalog(catalog));
        return 1;
    }
    let cards = parse_cards(text);
    let gradeable = gradeable_cards(cards);
    baml.io.println("cards: " + i2s(cards.length()) + ", gradeable: " + i2s(gradeable.length()));
    for (let c in cards) {
        baml.io.println("  " + c.id + " [" + c.status + " w=" + f2(c.weight) + "] " + c.title);
    }
    0
}
