// How high each rule reaches: the most type-system rules a case can hold when
// that rule is its conclusion. Derived from what the bank generates rather
// than declared beside each rule, so it cannot drift from the facts.

/// A rule's order, over the first `seeds` seeds of every item a sitting can
/// serve. Only verified items count, and only rules they conclude are
/// listed: a rule that reaches a learner solely through an item the compiler
/// disagrees with is not one a learner can be asked about, so it has no
/// place in what we expect them to master.
function rule_orders(seeds: int) -> root.engine.RuleOrder[] throws never {
    let servable = items().filter((item) -> {
        item.status() is root.engine.Verified
    });
    let traces = servable.flat_map((item) -> {
        let id = item.id();
        baml.iter.Range.new(0, seeds)
            .filter_map((i) -> {
                let candidate = root.engine.generate(item, root.engine.seed_for(id, i))
                    ?? return null;
                // The claims the answer TURNS ON, which is what the learner
                // is credited with when they get the case right. Measuring
                // the whole trace instead would set an order no sitting can
                // reach, and a rule whose order cannot be reached can never
                // be mastered and never stops being shaky.
                let own: string[] = root.engine.rules_of(candidate.case).filter((name) -> {
                    !is_site_rule(name)
                });
                own
            })
            .collect()
    });
    rules().filter_map((rule) -> {
        if (is_site_rule(rule.name)) {
            return null;
        }
        let orders = traces.filter_map((trace) -> {
            if (trace.at(-1) == rule.name) {
                trace.length()
            } else {
                null
            }
        });
        let sorted = orders.sort_by_key((n) -> {
            n
        });
        let highest = sorted.at(-1)
            ?? return null;
        let lowest = sorted.at(0)
            ?? return null;
        root.engine.RuleOrder { rule: rule.name, lowest, highest }
    })
}
