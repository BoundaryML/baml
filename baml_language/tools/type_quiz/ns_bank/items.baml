// Items: what turns a fact into a case a learner sees.

/// The diagnostic a rejected flow produces at `site`. Compiler surface, so
/// bank data; today every site reports the same mismatch code.
function rejection_code(site: root.algebra.SiteKind) -> string throws never {
    match (site) {
        root.algebra.SiteKind.Let => "E0001",
        root.algebra.SiteKind.CallArgument => "E0001",
        root.algebra.SiteKind.Return => "E0001",
        root.algebra.SiteKind.Field => "E0001",
        root.algebra.SiteKind.ArrayElement => "E0001",
    }
}

/// A derivation's pair oriented as a flow: a value of `source` meets a slot
/// of `target`, and `relation` is how `source` relates to `target`.
class Oriented {
    source: root.algebra.Ty,
    target: root.algebra.Ty,
    relation: root.algebra.Relation,
    reversed: bool,
}

/// `d`'s pair flowing forwards (left into right) or, `reversed`, backwards.
function orient(
    d: root.algebra.Derivation,
    relation: root.algebra.Relation,
    reversed: bool,
) -> Oriented throws never {
    if (reversed) {
        Oriented {
            source: root.algebra.right_of(d),
            target: root.algebra.left_of(d),
            relation: root.algebra.mirror(relation),
            reversed,
        }
    } else {
        Oriented {
            source: root.algebra.left_of(d),
            target: root.algebra.right_of(d),
            relation,
            reversed,
        }
    }
}

/// `relation`, which holds between the derivation's left and right, read in
/// the flow's direction.
function in_direction(
    relation: root.algebra.Relation,
    flow: Oriented,
) -> root.algebra.Relation throws never {
    if (flow.reversed) {
        root.algebra.mirror(relation)
    } else {
        relation
    }
}

/// The flow rule at this case: which type meets which slot, at which site,
/// and what follows.
function flow_claim(
    site: root.algebra.SiteKind,
    flow: Oriented,
    compiles: bool,
) -> root.engine.Claim throws never {
    let s = `\`${root.algebra.render(flow.source)}\``;
    let t = `\`${root.algebra.render(flow.target)}\``;
    let outcome = if (compiles) {
        "holds, so the program compiles"
    } else {
        "does not hold, so the program is rejected"
    };
    root.engine.Claim {
        rule: flow_requires_subtype(),
        instance: `at ${root.algebra.site_phrase(site)}, a value of type ${s} meets a slot of type ${t}: ${s} <: ${t} ${outcome}`,
    }
}

/// What a seed settles before either direction of a case is rendered.
///
/// Both directions are spelled from one of these, so they can differ in
/// nothing but the swap: the names in particular are drawn from the pair
/// itself rather than from the direction it flows in.
class Drawn {
    d: root.algebra.Derivation,
    relation: root.algebra.Relation,
    /// Which way this seed flows the pair.
    reversed: bool,
    site: root.algebra.SiteKind,
    names: root.algebra.SiteNames,
}

function draw_flow(
    fact: Fact,
    sites: root.algebra.SiteKind[],
    seed: root.engine.Seed,
) -> Drawn? throws never {
    let streams = root.engine.split(seed);
    let d = fact.generate(streams.left).value;
    let relation = root.algebra.decided(root.algebra.knowledge(d))
        ?? return null;
    let direction = root.engine.below(streams.right, 2);
    let site_draw = root.engine.below(direction.next, sites.length());
    let reserved = root.algebra.decls_of_all([root.algebra.left_of(d), root.algebra.right_of(d)]).map((decl) -> {
        root.algebra.decl_name(decl)
    });
    Drawn {
        d,
        relation,
        reversed: direction.value == 1,
        site: sites[site_draw.value],
        names: site_names(site_draw.next, reserved),
    }
}

/// One direction of a fact at a site: the program it renders, what the
/// compiler must say about it, and why.
class Flowed {
    flow: Oriented,
    case: root.engine.Case,
    compiles: bool,
}

/// Whether a relation reads differently in the two directions, so the two
/// programs a fact renders answer differently and can be put side by side.
/// It looks only at the relation: an equivalence and an unrelated pair read
/// the same both ways.
function pairs(relation: root.algebra.Relation) -> bool throws never {
    root.algebra.flows(relation) != root.algebra.flows(root.algebra.mirror(relation))
}

/// The pair of `drawn` flowing one way at its site, as a case.
function flowed(drawn: Drawn, reversed: bool) -> Flowed throws never {
    let flow = orient(drawn.d, drawn.relation, reversed);
    let probe = root.algebra.probe(drawn.site, flow.source, flow.target, drawn.names);
    let compiles = root.algebra.flows(flow.relation);
    let file = "case.baml";
    let key: root.engine.Verdict = if (compiles) {
        root.engine.Compiles { tests: [] }
    } else {
        root.engine.Rejected {
            code: rejection_code(drawn.site),
            more: [],
            at: root.engine.Region { file, start: probe.start, end: probe.end },
        }
    };
    let trace = root.algebra.claims(drawn.d).concat([flow_claim(drawn.site, flow, compiles)]);
    Flowed {
        flow,
        case: root.engine.Case { files: root.engine.one_file(file, probe.source), key, trace },
        compiles,
    }
}

/// The naive models that read one program's verdict the other way.
function flow_traps(d: root.algebra.Derivation, one: Flowed) -> string[] throws never {
    models().filter_map((model) -> {
        let believed = root.algebra.decided(root.algebra.under(d, model))
            ?? return null;
        if (root.algebra.flows(in_direction(believed, one.flow)) != one.compiles) {
            model.id
        } else {
            null
        }
    })
}

/// What differs between the two directions of one fact, in the learner's
/// terms. It says which two types swap places and not which way round the
/// compiler wants them: that is the question.
function flow_difference(site: root.algebra.SiteKind, one: Flowed) -> string throws never {
    let a = `\`${root.algebra.render(one.flow.source)}\``;
    let b = `\`${root.algebra.render(one.flow.target)}\``;
    `the same program twice with the two types swapped: at ${root.algebra.site_phrase(site)}, one of them lets a value of type ${a} meet a slot of type ${b}, and the other a value of type ${b} meet a slot of type ${a}`
}

/// The naive models that would pick the wrong one of two directions.
///
/// A model picks the direction its own beliefs let flow. Where its beliefs
/// let both flow, or neither, it has no pick to be wrong about: a choice
/// does not trap a model that a single program does, and crediting it would
/// hold a learner's answer against a belief that could not have produced it.
function choice_traps(
    d: root.algebra.Derivation,
    one: Flowed,
    other: Flowed,
) -> string[] throws never {
    models().filter_map((model) -> {
        let believed = root.algebra.decided(root.algebra.under(d, model))
            ?? return null;
        let flows_one = root.algebra.flows(in_direction(believed, one.flow));
        let flows_other = root.algebra.flows(in_direction(believed, other.flow));
        if (flows_one == flows_other || flows_one == one.compiles) {
            null
        } else {
            model.id
        }
    })
}

/// A fact exercised at a flow site. Half the seeds flow the pair forwards
/// and half backwards, so a strict-subtype fact yields a compiling case as
/// often as a rejected one and the shape never gives the verdict away. The
/// direction is part of the explanation: the flow claim names which type
/// meets which slot.
class FlowItem {
    fact: Fact,
    status: root.engine.Status,
    sites: root.algebra.SiteKind[],

    implements root.engine.Item {
        function id(self) -> string throws never {
            `flow/${self.fact.name}`
        }

        function status(self) -> root.engine.Status throws never {
            self.status
        }

        function generate(self, seed: root.engine.Seed) -> root.engine.Candidate? throws never {
            let drawn = draw_flow(self.fact, self.sites, seed)
                ?? return null;
            let chosen = flowed(drawn, drawn.reversed);
            let traps = flow_traps(drawn.d, chosen);
            root.engine.Candidate {
                case: chosen.case,
                features: features_of(
                    chosen.case.trace,
                    root.algebra.flips(drawn.d),
                    [chosen.flow.source, chosen.flow.target],
                    traps,
                ),
                traps,
            }
        }

        function foil(self, seed: root.engine.Seed) -> root.engine.Foil? throws never {
            let drawn = draw_flow(self.fact, self.sites, seed)
                ?? return null;
            // Exactly one direction of a strict relation compiles, so a
            // strict fact, and only a strict fact, has a second program to
            // offer. That the two do answer differently is held to by the
            // suite, which reads it off the two keys.
            if (!pairs(drawn.relation)) {
                return null;
            }
            let chosen = flowed(drawn, drawn.reversed);
            let other = flowed(drawn, !drawn.reversed);
            root.engine.Foil {
                case: other.case,
                difference: flow_difference(drawn.site, chosen),
                traps: choice_traps(drawn.d, chosen, other),
            }
        }
    }
}

/// `type B = B;` and its two-alias form, which the spec rejects with E0068.
/// The compiler accepts both today: see COMPILER_DIVERGENCE.md, CD-001.
class UnguardedCycleItem {
    implements root.engine.Item {
        function id(self) -> string throws never {
            "aliases/unguarded-cycle"
        }

        function status(self) -> root.engine.Status throws never {
            root.engine.CompilerDiverges {
                divergence: "CD-001",
                contradiction: root.engine.Contradiction.Accepts,
            }
        }

        function generate(self, seed: root.engine.Seed) -> root.engine.Candidate? throws never {
            let names = root.engine.pick_distinct(seed, type_names(), 2);
            let form = root.engine.below(names.next, 2);
            let a = names.value[0];
            let b = names.value[1];
            let decl = if (form.value == 0) {
                root.algebra.AliasDecl.recursive(a, (cycle) -> {
                    cycle
                })
            } else {
                root.algebra.AliasDecl.recursive(a, (cycle) -> {
                    root.algebra.Alias { decl: root.algebra.AliasDecl { name: b, body: cycle } }
                })
            };
            let ty: root.algebra.Ty = root.algebra.Alias { decl };
            let source = root.algebra.render_decls(root.algebra.decls_of(ty));
            let file = "case.baml";
            // The spec's E0068 is per cycle member, so the two-alias form
            // expects two. Where the compiler will point is the first alias's
            // name: its first occurrence in the source is its declaration.
            let more: string[] = if (form.value == 0) {
                []
            } else {
                ["E0068"]
            };
            let prefix = source.slice(0, source.index_of(a) ?? 0);
            let start = prefix.to_utf8().length();
            let trace = [
                root.engine.Claim {
                    rule: unguarded_cycle_rejected(),
                    instance: `\`${a}\` is a fully unguarded cycle: it denotes \`never\` and is rejected`,
                },
            ];
            root.engine.Candidate {
                case: root.engine.Case {
                    files: root.engine.one_file(file, source),
                    key: root.engine.Rejected {
                        code: "E0068",
                        more,
                        at: root.engine.Region { file, start, end: start + a.to_utf8().length() },
                    },
                    trace,
                },
                features: features_of(trace, 0, [ty], []),
                traps: [],
            }
        }

        /// A cycle has no neighbouring program that compiles: what would
        /// have to change to make one is the whole of what the case is
        /// about.
        function foil(self, seed: root.engine.Seed) -> root.engine.Foil? throws never {
            null
        }
    }
}
