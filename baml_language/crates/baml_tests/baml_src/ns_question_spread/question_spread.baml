// Regression for B-1788: compiling this function must not crash, even when
// execution returns before the sort and final class construction.
class Dispatch {
    id: string,
    sessionId: string,
    userId: string,
    atMs: int,
    kind: string,
    description: string,
    prompt: string,
    report: string,
    areas: map<string, int>,
    calls: int,
}

class QuestionSpread {
    areas: int,
    topArea: string,
    topShare: float,
    shape: string,
}

function question_spread(ids: string[], dispatches: Dispatch[]) -> QuestionSpread {
    let calls: map<string, int> = {};
    for (let d in dispatches) {
        if (ids.includes(d.id)) {
            for (let area in d.areas.keys()) {
                calls.set(area, (calls.get(area) ?? 0) + (d.areas.get(area) ?? 0));
            }
        }
    }
    let total = calls.values().reduce(
        (sum, n) -> {
            sum + n
        },
        0,
    );
    if (total == 0) {
        return QuestionSpread { areas: 0, topArea: "", topShare: 0.0, shape: "unknown" };
    }
    let ranked = calls.keys();
    ranked.sort_by_key((a) -> {
        0 - (calls.get(a) ?? 0)
    });
    let needed = 0;
    let covered = 0;
    for (let a in ranked) {
        needed += 1;
        covered += calls.get(a) ?? 0;
        if (covered * 5 >= total * 4) {
            break;
        }
    }
    let top_share = (calls.get(ranked[0]) ?? 0) / (total * 1.0);
    QuestionSpread {
        areas: needed,
        topArea: ranked[0],
        topShare: (top_share * 100.0).round() / 100.0,
        shape: if (needed >= 3 && top_share < 0.5) {
            "spanning"
        } else {
            "local"
        },
    }
}

test "spread" {
    assert.equal(question_spread(["d1"], []).shape, "unknown")
}

function dispatch(id: string, areas: map<string, int>) -> Dispatch {
    Dispatch {
        id: id,
        sessionId: "session",
        userId: "user",
        atMs: 0,
        kind: "question",
        description: "",
        prompt: "",
        report: "",
        areas: areas,
        calls: 0,
    }
}

test "spread ignores unselected dispatches" {
    let result = question_spread(["d1"], [dispatch("d2", { "other": 10 })]);
    assert.equal(result.areas, 0);
    assert.equal(result.topArea, "");
    assert.equal(result.topShare, 0.0);
    assert.equal(result.shape, "unknown");
}

test "spread local" {
    let result = question_spread(["d1"], [dispatch("d1", { "top": 9, "other": 1 })]);
    assert.equal(result.areas, 1);
    assert.equal(result.topArea, "top");
    assert.equal(result.topShare, 0.9);
    assert.equal(result.shape, "local");
}

test "spread spanning" {
    let result = question_spread(
        ["d1", "d2"],
        [
            dispatch("d1", { "top": 3, "second": 3, "third": 2, "fourth": 1 }),
            dispatch("d2", { "top": 1, "fourth": 1 }),
            dispatch("ignored", { "other": 100 }),
        ],
    );
    assert.equal(result.areas, 3);
    assert.equal(result.topArea, "top");
    assert.equal(result.topShare, 0.36);
    assert.equal(result.shape, "spanning");
}

function identity<T>(value: T) -> T {
    value
}

test "deferred arithmetic method receiver through generic aliases" {
    let later = [];
    let total = [1, 2].reduce(
        (sum, n) -> {
            sum + n
        },
        0,
    );
    assert.equal(identity(identity(total * 1.0)).round(), 3.0);
    // Demanding the receiver must not finalize unrelated inference variables.
    later.push("hello");
    assert.equal(later[0], "hello");
}
