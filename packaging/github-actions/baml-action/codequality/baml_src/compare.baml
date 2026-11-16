/// The smallest change in cognitive or cyclomatic complexity worth a table row.
const BRANCH_STEP = 3n;

/// The smallest change in nesting depth or exit count worth a table row.
const DEPTH_STEP = 2n;

/// The smallest change in NPath worth a table row, on top of doubling or
/// halving: NPath multiplies, so 1 to 2 paths is noise.
const NPATH_STEP = 4n;

class MetricChange {
    metric: Metric,
    before: bigint,
    after: bigint,

    /// Whether the metric moved enough to be worth a reviewer's attention.
    function significant(self) -> bool {
        let smaller = self.before.min(self.after);
        let larger = self.before.max(self.after);
        let step = larger - smaller;
        match (self.metric) {
            Metric.NPath => larger >= smaller * 2n && step >= NPATH_STEP,
            Metric.Nesting | Metric.Exits => step >= DEPTH_STEP,
            Metric.Cognitive | Metric.Cyclomatic => step >= BRANCH_STEP,
        }
    }

    /// How much the metric grew, as `after / before` from zero up counting as
    /// from one.
    function growth(self) -> Ratio {
        Ratio.new(self.after, self.before.max(1n))
    }

    /// How much the metric shrank, the inverse of `growth`.
    function shrinkage(self) -> Ratio {
        Ratio.new(self.before, self.after.max(1n))
    }

    implements baml.ToString {
        function to_string(self) -> string {
            `${metric_word(self.metric)} ${count(self.before)} &rarr; ${count(self.after)}`
        }
    }
}

/// A function in both reports, and every metric of it that changed.
class FunctionChange {
    report: FunctionReport,
    changes: MetricChange[],

    function between(before: FunctionReport, after: FunctionReport) -> Self {
        FunctionChange {
            report: after,
            changes: ALL_METRICS.filter_map((m) -> {
                let change = MetricChange { metric: m, before: before.value(m), after: after.value(m) };
                if (change.before != change.after) {
                    change
                } else {
                    null
                }
            }),
        }
    }

    /// A function is worse if any metric went up, and better if metrics only
    /// went down.
    function worse(self) -> bool {
        self.changes.some((c) -> {
            c.after > c.before
        })
    }

    function significant(self) -> bool {
        self.changes.some((c) -> {
            c.significant()
        })
    }

    /// How far the most-changed metric moved in the function's direction.
    function largest_move(self) -> Ratio {
        let moves = self.changes.map((c) -> {
            if (self.worse()) {
                c.growth()
            } else {
                c.shrinkage()
            }
        });
        moves.reduce(
            (largest, r) -> {
                largest.max(r)
            },
            Ratio.new(0n, 1n),
        )
    }

    implements baml.ToString {
        function to_string(self) -> string {
            let direction = if (self.worse()) {
                "**Worse**"
            } else {
                "**Better**"
            };
            `${direction}: ${self.changes.map((c) -> { c.to_string() }).join(", ")}`
        }
    }
}

/// Biggest move first, then by name and file so the order is stable.
function by_largest_move(changes: FunctionChange[]) -> FunctionChange[] {
    changes.sort_by((a, b) -> {
        let by_move = b.largest_move().cmp(a.largest_move());
        match (by_move) {
            baml.ops.Ordering.Equal => a.report.key().cmp(b.report.key()),
            _ => by_move,
        }
    })
}

/// The base could not be measured, so there is nothing to compare against.
class Unmeasured {
}

type Outcome = Comparison | Unmeasured;

class Comparison {
    worse: FunctionChange[],
    better: FunctionChange[],
    added: FunctionReport[],
    removed: int,
    before: Summary,
    after: Summary,

    function new(base: Report, head: Report) -> Self {
        let base_functions = base.functions_by_key();
        let head_functions = head.functions_by_key();
        let changed = head_functions.values()
            .filter_map((after) -> {
                match (base_functions.get(after.key())) {
                    null => null,
                    let before: FunctionReport => FunctionChange.between(before, after),
                }
            })
            .filter((c) -> {
                c.changes.length() > 0
            });
        Comparison {
            worse: by_largest_move(
                changed.filter((c) -> {
                    c.worse()
                }),
            ),
            better: by_largest_move(
                changed.filter((c) -> {
                    !c.worse()
                }),
            ),
            added: head_functions.values()
                .filter((f) -> {
                    !base_functions.has(f.key())
                })
                .sort_by((a, b) -> {
                    match (b.cognitive.cmp(a.cognitive)) {
                        baml.ops.Ordering.Equal => a.name.cmp(b.name),
                        let by_cognitive => by_cognitive,
                    }
                }),
            removed: base_functions.keys()
                .filter((key) -> {
                    !head_functions.has(key)
                })
                .length(),
            before: base.summary(),
            after: head.summary(),
        }
    }

    function changed(self) -> bool {
        self.worse || self.better || self.added || self.removed
    }

    /// The first line: how many functions got worse or better, how many came
    /// and went, and how the project's size changed.
    function headline(self) -> string {
        [self.verdict(), self.arrivals(), self.size_change()]
            .filter_map((sentence) -> {
                sentence
            })
            .join(" ")
    }

    function verdict(self) -> string {
        if (!self.changed()) {
            return "**No function changed complexity.**";
        }
        let worse = match (self.worse.length()) {
            0 => "**No function got worse**",
            let n => `**${plural(n, "function")} got worse**`,
        };
        if (self.better) {
            return `${worse}, ${self.better.length()} got better.`;
        }
        `${worse}.`
    }

    /// Functions added and removed; null when neither happened.
    function arrivals(self) -> string? {
        let parts = [
            if (self.added) {
                `${self.added.length()} new`
            } else {
                null
            },
            if (self.removed) {
                `${self.removed} removed`
            } else {
                null
            },
        ]
            .filter_map((part) -> {
                part
            });
        if (!parts) {
            return null;
        }
        `${parts.join(", ")}.`
    }

    /// The line count before and after; null when it did not change.
    function size_change(self) -> string? {
        if (self.before.lines == self.after.lines) {
            return null;
        }
        `Lines ${count(self.before.lines)} &rarr; ${count(self.after.lines)}.`
    }
}
