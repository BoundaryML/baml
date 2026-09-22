// Shape tells: whether the surface of a case predicts its answer.
//
// A learner can read a program's surface — which constructs it uses, which
// declarations it makes, where the pair is put — without reasoning about the
// type system. If any of that predicts the compiler's answer, the quiz is
// measuring pattern-matching on the bank rather than the type system, and a
// learner who has noticed that every recursive alias compiles is credited
// with rules they never applied. That was the bank as first built: a fact
// that concludes an equivalence compiles whichever way it flows and one that
// concludes an unrelated pair never does, so each construct leaned exactly as
// far as the facts it lived in — a recursive alias compiled 93 times in 100,
// a `map<` 19.
//
// The bar: every feature of the surface within ten points of how often any
// case compiles, and that within five of half. It is a property of the bank
// and has to be, because selection cannot buy it: forcing the answers even
// forces the items uneven.
//
// It is NOT met. Near misses and the two-kinds-of-inner-pair wrappers carry
// the per-construct features, but two features of the PAIR — the two types
// written the same way, and one spelling inside the other — lean 40 and 25
// points, and the test below records them rather than claiming otherwise.
// They went unseen until `pair_features` named them: every other feature
// here describes one type at a time, and no number of those can say the two
// are written alike, which is the first thing a reader sees.
//
// What counts as surface is what is on screen before the answer. The rule a
// case turns on is not: it is named in the explanation, after. So the rules,
// the facts and the relations are kept in the sweep as the diagnosis of a
// lean and are not themselves held to the bar.
//
// The sweep mirrors `trap_faults`: every fact, at the site and in the
// direction each seed draws, which is exactly the case `generate` serves.

/// One case of the sweep, reduced to its surface and its answer.
class Surfaced {
    fact: string,
    compiles: bool,
    /// What a learner sees before answering.
    features: string[],
}

function note(out: string[], feature: string) -> void throws never {
    if (!out.includes(feature)) {
        out.push(feature);
    }
}

/// Every construct a learner would see in the spelling of `tys` and the
/// declarations they need, as feature names. Alias bodies are walked once
/// each, so a recursive alias is a feature rather than a loop.
function gather_kinds(ty: root.algebra.Ty, seen: string[], out: string[]) -> void throws never {
    match (ty) {
        let p: root.algebra.Prim => {
            note(out, "kind:prim");
            note(out, `prim:${root.algebra.render(p)}`);
        },
        root.algebra.Never => note(out, "kind:never"),
        root.algebra.Top => note(out, "kind:unknown"),
        let l: root.algebra.Lit => {
            note(out, "kind:literal");
            match (l.value) {
                let i: int => note(out, "lit:int"),
                let s: string => note(out, "lit:string"),
                let b: bool => note(out, "lit:bool"),
            }
        },
        root.algebra.EnumLit => {
            note(out, "kind:enum");
            note(out, "kind:enum-literal");
        },
        let u: root.algebra.Union => {
            note(out, "kind:union");
            let has_null = u.members().some((m) -> {
                match (m) {
                    let p: root.algebra.Prim => p.kind == root.algebra.PrimKind.Null,
                    root.algebra.Never
                        | root.algebra.Top
                        | root.algebra.Lit
                        | root.algebra.EnumLit
                        | root.algebra.Union
                        | root.algebra.Array
                        | root.algebra.Map
                        | root.algebra.Fn
                        | root.algebra.Class
                        | root.algebra.Enum
                        | root.algebra.Iface
                        | root.algebra.Alias
                        | root.algebra.Var =>
                    {
                        false
                    },
                }
            });
            if (has_null) {
                note(out, "kind:optional");
            }
        },
        root.algebra.Array => note(out, "kind:array"),
        root.algebra.Map => note(out, "kind:map"),
        let f: root.algebra.Fn => {
            note(out, "kind:function");
            if (
                f.named.some((n) -> {
                    n.optional
                })
            ) {
                note(out, "kind:named-optional");
            }
        },
        let c: root.algebra.Class => {
            note(out, "kind:class");
            if (c.args.length() > 0) {
                note(out, "kind:generic-args");
            }
            if (c.decl.impls.length() > 0) {
                note(out, "decl:implements");
            }
            if (!seen.includes(c.decl.name)) {
                seen.push(c.decl.name);
                for (let field in c.decl.fields) {
                    gather_kinds(field.ty, seen, out);
                }
            }
        },
        root.algebra.Enum => note(out, "kind:enum"),
        let i: root.algebra.Iface => {
            note(out, "kind:interface");
            if (i.pins.length() > 0) {
                note(out, "kind:pins");
            }
            if (i.decl.bounds.length() > 0) {
                note(out, "decl:requires");
            }
        },
        let a: root.algebra.Alias => {
            if (root.algebra.is_recursive(a.decl)) {
                note(out, "kind:alias-recursive");
            } else {
                note(out, "kind:alias");
            }
            if (!seen.includes(a.decl.name)) {
                seen.push(a.decl.name);
                gather_kinds(a.decl.body, seen, out);
            }
        },
        let v: root.algebra.Var => {
            note(out, "kind:typevar");
            match (v.bound) {
                let b: root.algebra.Iface => {
                    note(out, "kind:typevar-bounded");
                    gather_kinds(b, seen, out);
                },
                null => {},
            }
        },
    }
    for (let child in root.algebra.children(ty)) {
        gather_kinds(child, seen, out);
    }
}

/// Words a learner would spot in the source itself, including what the site
/// scaffolds around the pair.
function text_features(source: string) -> string[] throws never {
    [
        "implement",
        "interface",
        "extends",
        "class ",
        "enum ",
        "type ",
        "->",
        "|",
        "[]",
        "map<",
        "?",
        "never",
        "unknown",
        "throws",
        "<",
    ]
        .filter((word) -> {
            source.includes(word)
        })
        .map((word) -> {
            `text:${word}`
        })
}

/// How the two types on screen stand to each other as spellings, which is
/// the first thing a reader sees and the last thing `gather_kinds` can say.
///
/// Every other feature describes one type at a time, so a case whose two
/// types are written the same way carries nothing to tell it from one whose
/// types merely share a construct. That is the most visible thing about a
/// case and it is not a fact about either type: it is a fact about the pair.
function pair_features(source: string, target: string) -> string[] throws never {
    if (source == target) {
        ["pair:same-spelling"]
    } else if (target.includes(source)) {
        ["pair:source-within-target"]
    } else if (source.includes(target)) {
        ["pair:target-within-source"]
    } else {
        []
    }
}

/// Every fact, at the site and direction each of `seeds` seeds draws.
function surfaced(seeds: int) -> Surfaced[] throws never {
    root.bank.facts().flat_map((fact) -> {
        baml.iter.Range.new(0, seeds)
            .filter_map((i) -> {
                let seed = root.engine.seed_for(`flow/${fact.name}`, i);
                let drawn = root.bank.draw_flow(fact, root.algebra.all_sites(), seed)
                    ?? return null;
                let one = root.bank.flowed(drawn, drawn.reversed);
                let out: string[] = [];
                let seen: string[] = [];
                note(out, `site:${root.algebra.site_phrase(drawn.site)}`);
                gather_kinds(one.cast.flow.source, seen, out);
                gather_kinds(one.cast.flow.target, seen, out);
                let spelled = pair_features(
                    root.algebra.render(one.cast.flow.source),
                    root.algebra.render(one.cast.flow.target),
                );
                for (let feature in spelled) {
                    note(out, feature);
                }
                for (let feature in text_features(source_of(one.case))) {
                    note(out, feature);
                }
                Surfaced { fact: fact.name, compiles: one.cast.compiles, features: out }
            })
            .collect()
    })
}

/// One feature's count and how it leans.
class Tell {
    feature: string,
    n: int,
    compiling: int,
}

/// How far a feature leans from the sweep, in points: the difference between
/// how often a case with it compiles and how often any case does. Zero means
/// seeing the feature tells a learner nothing about the answer, which is the
/// property wanted; accuracy would not do, since a feature that only agrees
/// with the majority answer is still a tell to whoever has spotted it.
function lean(tell: Tell, total: int, compiling: int) -> int throws never {
    (percent_of(tell.compiling, tell.n) - percent_of(compiling, total)).abs()
}

/// Which facts carry `feature`, and how each answers: what a lean is made of,
/// and so where to add the missing half.
function carriers(cases: Surfaced[], feature: string) -> string throws never {
    let with_f = cases.filter((c) -> {
        c.features.includes(feature)
    });
    let none: string[] = [];
    let names = with_f
        .map((c) -> {
            c.fact
        })
        .reduce(
            (acc, name) -> {
                if (acc.includes(name)) {
                    acc
                } else {
                    acc.concat([name])
                }
            },
            none,
        );
    names
        .map((name) -> {
            let of = with_f.filter((c) -> {
                c.fact == name
            });
            let compiled = of
                .filter((c) -> {
                    c.compiles
                })
                .length();
            `${name} ${percent_of(compiled, of.length())}% of ${of.length()}`
        })
        .join(", ")
}

/// The base rate, and every feature of the surface that leans more than
/// `bar` points from it over at least `support` cases. Nothing leaning is the
/// property; anything listed is a shape a learner can answer from.
///
/// `carried` adds the facts carrying each lean and how each of them answers,
/// which is where a lean says what it is made of and what would balance it.
/// It is off where a lean is pinned: the carriers are a dozen counts that
/// move whenever any of them is redrawn, and a pin that churns on every
/// unrelated change stops being read.
function leaning_features(
    seeds: int,
    support: int,
    bar: int,
    carried: bool,
) -> string throws never {
    leaning_of(surfaced(seeds), support, bar, carried)
}

/// `leaning_features` over any set of cases: the bank's sweep, or what a
/// sitting actually served. `carried` adds, to each leaning feature, the
/// facts that carry it and how each answers — the diagnosis, which is worth
/// having in a failure and too long to pin where a lean is being recorded.
function leaning_of(
    cases: Surfaced[],
    support: int,
    bar: int,
    carried: bool,
) -> string throws never {
    let total = cases.length();
    let compiling = cases
        .filter((c) -> {
            c.compiles
        })
        .length();
    let counts: map<string, Tell> = {};
    for (let c in cases) {
        for (let feature in c.features) {
            let prior = counts.get(feature) ?? Tell { feature, n: 0, compiling: 0 };
            counts.set(
                feature,
                Tell {
                    feature,
                    n: prior.n + 1,
                    compiling: prior.compiling
                        + if (c.compiles) {
                            1
                        } else {
                            0
                        },
                },
            );
        }
    }
    let leaning = counts.values()
        .filter((t) -> {
            t.n >= support && lean(t, total, compiling) > bar
        })
        .sort_by_key((t) -> {
            // Furthest lean first, then the better supported.
            (100 - lean(t, total, compiling)) * 100000 + (total - t.n)
        })
        .map((t) -> {
            let leans = `${t.feature} ${percent_of(t.compiling, t.n)}% of ${t.n}`;
            if (carried) {
                `${leans} <- ${carriers(cases, t.feature)}`
            } else {
                leans
            }
        });
    `compiles=${percent_of(compiling, total)}% of ${total} leaning=${leaning}`
}

test "tells: the surface leans in the two places recorded here and nowhere else" {
    // RECORDED DEBT, not a property met. Two features of the pair do predict
    // the answer, and both are among the most visible things about a case:
    //
    //   pair:same-spelling      the two types are written the same way, which
    //                           at a flow site can only compile. It is not
    //                           100% only because `identity` and `same_class`
    //                           also reach the coherence site, where being
    //                           one type is what is rejected; nothing flips
    //                           `same_positional_count`, whose function types
    //                           are no implementation target, and it is 100%
    //                           of 128.
    //   pair:source-within-target
    //                           one spelling appears inside the other. This
    //                           is the subset rule where the containment is
    //                           real, and an accident of naming where it is
    //                           not — `Side` reads as inside `Side.Left`,
    //                           which is why that fact answers the other way.
    //
    // Both were invisible until `pair_features` named them, because every
    // other feature here describes ONE type and these are facts about the
    // pair. The bar they break is the bank's own, and the fix is the bank's:
    // an equivalence spelled the same way cannot be balanced by a near miss,
    // since a near miss spelled the same way would be the same type.
    //
    // Pinned so that a THIRD lean, or either of these getting worse, fails.
    // Pass `carried: true` for the facts behind each.
    assert.equal(
        leaning_features(128, 100, 10, false),
        `compiles=50% of 7424 leaning=["pair:same-spelling 90% of 440", "pair:source-within-target 75% of 835"]`,
    )
}

// ─── what selection can see ──────────────────────────────────────────────────
//
// A bank that says nothing with its surface is not enough. Selection prefers
// some cases to others, and what a case is decides both its answer and why
// selection would want it. The naive models are lopsided — one that accepts
// too much is wrong only about rejected cases, one that accepts too little
// only about compiling ones — so a selection that could see which direction
// of a draw fools somebody served that direction: function types rejected
// ten times in eleven. And the tracer wants evidence per rule, and a rule
// that concludes an equivalence can only be evidenced by a case that
// compiles, so a selection that could see which fact of a shape it had drawn
// served the equivalences: unions accepted seven times in ten. Both from a
// bank in which every kind of question answers both ways.
//
// So selection reads a draw's castings — every way its coins could have
// served it — and never the case. What is held here is that it cannot do
// otherwise: whichever way the coins fall, what it makes of the draw is the
// same, number for number.

/// A profile with something to say: a short sitting of the learner who
/// reasons as TypeScript does, so its rules stand unevenly and one model is
/// suspected. A fresh profile holds every rule alike and would let a
/// selection that read the served casting look blind by accident.
function telling_profile() -> root.engine.Profile throws never {
    let knobs = root.engine.Knobs { ...root.engine.default_knobs(), practice: true, budget: 12 };
    sit(typescripter, knobs, 5).profile
}

/// What selection makes of `candidate` drawn for `item` at `seed`: every
/// number it would act on.
function selection_view(
    item: string,
    seed: root.engine.Seed,
    candidate: root.engine.Candidate,
    profile: root.engine.Profile,
) -> string throws never {
    let score = root.engine.score_of(candidate, root.bank.default_weights())
        ?? return "capped";
    let scored = root.engine.Scored { item, seed, candidate, score };
    `score=${score} fit=${root.engine.fit(profile, root.engine.default_knobs(), scored)}`
}

/// Every draw of every item that selection could tell apart from itself: a
/// way its coins could fall under which selection would make something
/// different of it. With how many draws had outcomes that DO differ in what
/// they were served as, so the check is known not to pass by having nothing
/// to see.
function outcomes_told_apart(seeds: int) -> string throws never {
    let profile = telling_profile();
    let found = root.bank.shapes().flat_map((shape) -> {
        let item = `flow/${shape.name}`;
        baml.iter.Range.new(0, seeds)
            .filter_map((i) -> {
                let seed = root.engine.seed_for(item, i);
                let dealt = root.bank.deal(shape, root.algebra.all_sites(), seed)
                    ?? return null;
                let outcomes = baml.iter.Range.new(0, dealt.all.length()).collect().flat_map((chosen) -> {
                    [false, true].map((reversed) -> {
                        root.bank.candidate_of(dealt.all, chosen, reversed)
                    })
                });
                let views = outcomes.map((candidate) -> {
                    selection_view(item, seed, candidate, profile)
                });
                let blind = views.every((view) -> {
                    view == views[0]
                })
                    && outcomes.every((candidate) -> {
                        candidate.castings == outcomes[0].castings
                    });
                let differ = outcomes.some((candidate) -> {
                    candidate.served != outcomes[0].served
                });
                if (blind) {
                    if (differ) {
                        "differs"
                    } else {
                        "same"
                    }
                } else {
                    `${shape.name}#${i}`
                }
            })
            .collect()
    });
    let seen_apart = found.filter((f) -> {
        f != "differs" && f != "same"
    });
    `told_apart=${seen_apart} outcomes_that_differ=${found.filter((f) -> {
        f == "differs"
    }).length()} of ${found.length()}`
}

test "selection: whichever way a draw's coins fall, it makes the same of it" {
    assert.equal(outcomes_told_apart(8), "told_apart=[] outcomes_that_differ=194 of 320")
}

/// Every draw of every item whose served casting is not what the tracer
/// reads off the case it served. The two are worked out apart — one from
/// the derivation before anything is rendered, the other from the case —
/// and selection's view is only honest while they agree.
function served_misread(seeds: int) -> string[] throws never {
    root.bank.items().flat_map((item) -> {
        let id = item.id();
        baml.iter.Range.new(0, seeds)
            .filter_map((i) -> {
                let candidate = root.engine.generate(item, root.engine.seed_for(id, i))
                    ?? return null;
                if (candidate.served.rules == root.tracer_rules(candidate.case)) {
                    null
                } else {
                    `${id}#${i}: ${candidate.served.rules} against ${root.tracer_rules(candidate.case)}`
                }
            })
            .collect()
    })
}

test "selection: a casting holds the rules the tracer reads off its case" {
    assert.equal(served_misread(8).join("\n"), "")
}
