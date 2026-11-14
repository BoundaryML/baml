// "Did you mean" candidates, chosen the way clap chooses them.

// Candidates must be more similar than this to be suggested.
const SUGGESTION_THRESHOLD = 0.7;

class Suggestions {
    // The candidates similar to `value`, least similar first.
    function did_you_mean(value: string, candidates: string[]) -> string[] {
        candidates
            .map((candidate: string) -> _Scored {
                _Scored { text: candidate, confidence: Suggestions.jaro(value, candidate) }
            })
            .filter((scored: _Scored) -> bool {
                scored.confidence > SUGGESTION_THRESHOLD
            })
            .sort_by_key((scored: _Scored) -> float {
                scored.confidence
            })
            .map((scored: _Scored) -> string {
                scored.text
            })
    }

    // Jaro similarity: 1.0 for equal strings, 0.0 for nothing in common.
    function jaro(left: string, right: string) -> float {
        let a = left.chars();
        let b = right.chars();
        if (a.length() == 0 && b.length() == 0) {
            return 1.0;
        }
        if (a.length() == 0 || b.length() == 0) {
            return 0.0;
        }
        // Characters match when equal and no further apart than this.
        let longer = if (a.length() > b.length()) {
            a.length()
        } else {
            b.length()
        };
        let range = if (longer / 2 > 0) {
            longer / 2 - 1
        } else {
            0
        };
        let a_matched = baml.Array.filled(a.length(), false);
        let b_matched = baml.Array.filled(b.length(), false);
        let matches = 0;
        let i = 0;
        while (i < a.length()) {
            let j = if (i > range) {
                i - range
            } else {
                0
            };
            while (j < b.length() && j <= i + range) {
                if (!b_matched[j] && a[i] == b[j]) {
                    a_matched[i] = true;
                    b_matched[j] = true;
                    matches += 1;
                    break;
                }
                j += 1;
            }
            i += 1;
        }
        if (matches == 0) {
            return 0.0;
        }
        // Matched characters that appear in a different order.
        let transpositions = 0;
        let k = 0;
        i = 0;
        while (i < a.length()) {
            if (a_matched[i]) {
                while (!b_matched[k]) {
                    k += 1;
                }
                if (a[i] != b[k]) {
                    transpositions += 1;
                }
                k += 1;
            }
            i += 1;
        }
        let m = matches * 1.0;
        (m / a.length() + m / b.length() + (m - transpositions / 2) / m) / 3.0
    }
}

class _Scored {
    text: string,
    confidence: float,
}
