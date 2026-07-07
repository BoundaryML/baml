// Stage 0 — parse design-principles.md into PrincipleCard[].
//
// Deterministic: cards are split on "### P-" headings in document order; status
// is the first "status: <word>" occurrence in the card body; weight is a fixed
// status -> float map (the catalog's own grading-weight table).

function status_weight(status: string) -> float {
    match (status) {
        "implemented" => 1.0,
        "accepted" => 0.8,
        // rejected/superseded card text already encodes the inversion (the
        // rejected design IS the anti-pattern), so they grade at reduced weight.
        "rejected" => 0.6,
        "superseded" => 0.6,
        // rationale-only: gradeable when code contradicts the recorded reasoning.
        "pending" => 0.3,
        "proposed" => 0.3,
        // directional: flagged in the report, never scored.
        "draft" => 0.0,
        _ => 0.0,
    }
}

// First "status: <word>" in the body, lowercased, stripped of markdown bold.
function parse_status(body: string) -> string {
    let cleaned = body.replace_all("**", "").to_lower_case();
    let idx = cleaned.index_of("status: ");
    if (idx < 0) {
        return "unknown";
    }
    let after = cleaned.substring(idx + 8, cleaned.length().min(idx + 40));
    let word = "";
    for (let ch in after.split("")) {
        if (is_ident_char(ch)) {
            word = word + ch;
        } else {
            if (word.length() > 0) {
                return word;
            }
        }
    }
    word
}

// Value of a "- **<field>**: ..." bullet in the card body (single line).
function parse_field(body: string, field: string) -> string {
    let marker = "**" + field + "**:";
    for (let line in body.lines()) {
        let t = line.trim();
        if (t.starts_with("- " + marker)) {
            return t.substring(("- " + marker).length(), t.length()).trim();
        }
    }
    ""
}

// Split the catalog markdown into cards. A card starts at a line beginning
// "### P-" and runs until the next "### " or "## " heading.
function parse_cards(markdown: string) -> PrincipleCard[] {
    let cards: PrincipleCard[] = [];
    let cur_header = "";
    let cur_body = "";
    let in_card = false;
    for (let raw_line in markdown.lines()) {
        // trim: the embedded catalog is a raw string that `baml fmt` re-indents,
        // so heading detection must not depend on column-0 alignment.
        let line = raw_line.trim();
        let is_card_start = line.starts_with("### P-");
        let is_section = line.starts_with("## ") || line.starts_with("# ");
        if (is_card_start || (is_section && in_card)) {
            if (in_card) {
                cards.push(finish_card(cur_header, cur_body));
            }
            in_card = is_card_start;
            cur_header = line;
            cur_body = "";
        } else {
            if (in_card) {
                cur_body = cur_body + line + "\n";
            }
        }
    }
    if (in_card) {
        cards.push(finish_card(cur_header, cur_body));
    }
    cards
}

function finish_card(header: string, body: string) -> PrincipleCard {
    // header: "### P-044-1: Interfaces are for open, extensible dispatch"
    let rest = header.substring(4, header.length());
    let colon = rest.index_of(":");
    let id = if (colon >= 0) {
        rest.substring(0, colon).trim()
    } else {
        rest.trim()
    };
    let title = if (colon >= 0) {
        rest.substring(colon + 1, rest.length()).trim()
    } else {
        ""
    };
    let status = parse_status(body);
    return PrincipleCard {
        id: id,
        title: title,
        status: status,
        weight: status_weight(status),
        trigger: parse_field(body, "Applicability trigger"),
        detectability: parse_field(body, "Detectability"),
        body: header + "\n" + body,
    };
}

// Cards that participate in routing/grading, in document order.
function gradeable_cards(cards: PrincipleCard[]) -> PrincipleCard[] {
    cards
        .filter(
            (c: PrincipleCard) -> bool {
                c.weight > 0.0
            },
        )
}

// Compact one-line index used by the router prompt.
function card_index(cards: PrincipleCard[]) -> string {
    let out = "";
    for (let c in cards) {
        out
            = out
                + c.id
                + " ["
                + c.status
                + "] "
                + c.title
                + " | trigger: "
                + squash_ws(c.trigger)
                + "\n";
    }
    out
}
