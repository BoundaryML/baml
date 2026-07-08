// Scoring weights and aggregation. A finding's weight:
//
//   weight = card.weight (BEP-status)  ×  kind_weight  ×  size_factor
//
// kind_weight expresses how load-bearing a block kind is for codebase quality;
// size_factor gives bigger blocks more say (log-scaled, capped) while leaves
// stay fixed at 1.

function kind_weight(kind: string) -> float {
    match (kind) {
        "llm_function" => 1.0,
        "function" => 1.0,
        "method" => 1.0,
        "class" => 0.9,
        "enum" => 0.9,
        "interface" => 0.9,
        "type_alias" => 0.8,
        "test" => 0.7,
        "file" => 0.6,
        "program" => 0.5,
        "config" => 0.5,
        // leaves
        "field" => 0.4,
        "variant" => 0.4,
        "param" => 0.4,
        "var" => 0.4,
        // interactions
        "reference" => 0.7,
        "by_level" => 0.6,
        "by_depth" => 0.5,
        _ => 0.5,
    }
}

// Log-shaped step over span_lines, capped at 2.0; leaves and interactions
// fixed 1.0. (Piecewise instead of log10 — no float math stdlib dependency.)
function size_factor(span_lines: int, is_leafish: bool) -> float {
    if (is_leafish) {
        return 1.0;
    }
    if (span_lines < 10) {
        1.0
    } else {
        if (span_lines < 30) {
            1.3
        } else {
            if (span_lines < 100) {
                1.6
            } else {
                if (span_lines < 300) {
                    1.8
                } else {
                    2.0
                }
            }
        }
    }
}

// The full weight of one graded (subject, card) pair.
function finding_weight(card_weight: float, kind: string, span_lines: int, is_interaction: bool) -> float {
    let leafish = is_interaction
        || kind == "field"
        || kind == "variant"
        || kind == "param"
        || kind == "var";
    card_weight * kind_weight(kind) * size_factor(span_lines, leafish)
}

class ScoreOut {
    adherence: float,
    commission: float,
}

// Weighted means over id-sorted findings (order-stable float sums).
function compute_scores(findings: Finding[]) -> ScoreOut {
    let wsum = 0.0;
    let gsum = 0.0;
    let cw = 0.0;
    let cg = 0.0;
    for (let f in findings) {
        wsum = wsum + f.weight;
        gsum = gsum + f.weight * ((f.grade) * 1.0);
        if (f.verdict != "neutral") {
            cw = cw + f.weight;
            cg = cg + f.weight * ((f.grade) * 1.0);
        }
    }
    let adherence = if (wsum > 0.0) {
        gsum / wsum
    } else {
        0.0
    };
    let commission = if (cw > 0.0) {
        cg / cw
    } else {
        adherence
    };
    return ScoreOut { adherence: adherence, commission: commission };
}
