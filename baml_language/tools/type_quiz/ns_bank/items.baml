// Items: what turns a fact into a case a learner sees.

/// The diagnostic a rejected case produces at `site`. Compiler surface, so
/// bank data; every flow site reports the same mismatch code, and overlapping
/// implementations have their own.
function rejection_code(site: root.algebra.SiteKind) -> string throws never {
    match (site) {
        root.algebra.SiteKind.Let => "E0001",
        root.algebra.SiteKind.CallArgument => "E0001",
        root.algebra.SiteKind.Return => "E0001",
        root.algebra.SiteKind.Field => "E0001",
        root.algebra.SiteKind.ArrayElement => "E0001",
        root.algebra.SiteKind.Coherence => "E0132",
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

/// The rule a site contributes: what the construct does with the relation
/// the fact established. It is the last claim of every case, and the one
/// thing in a trace that is not the type-system content being asked about.
function site_rule(site: root.algebra.SiteKind) -> root.engine.Rule throws never {
    match (site) {
        root.algebra.SiteKind.Let
            | root.algebra.SiteKind.CallArgument
            | root.algebra.SiteKind.Return
            | root.algebra.SiteKind.Field
            | root.algebra.SiteKind.ArrayElement =>
        {
            flow_requires_subtype()
        },
        root.algebra.SiteKind.Coherence => coherence_at_most_one(),
    }
}

/// Whether `name` is a rule some site contributes. Read off the sites
/// themselves, so a new site's rule cannot be left out of the reckoning.
function is_site_rule(name: string) -> bool throws never {
    root.algebra.all_sites().some((site) -> {
        site_rule(site).name == name
    })
}

/// The site's own rule at this case: which type met which slot and what
/// follows, or what two `implements` blocks make of the pair.
function site_claim(
    site: root.algebra.SiteKind,
    flow: Oriented,
    compiles: bool,
) -> root.engine.Claim throws never {
    let s = `\`${root.algebra.render(flow.source)}\``;
    let t = `\`${root.algebra.render(flow.target)}\``;
    let instance = match (site) {
        root.algebra.SiteKind.Coherence => {
            let same = if (compiles) {
                "are not the same type, so the two blocks cannot both apply to one type and the program compiles"
            } else {
                "are the same type, so the second block implements what the first already did and the program is rejected"
            };
            `${root.algebra.site_phrase(site)}, one for ${s} and one for ${t}: they ${same}`
        },
        root.algebra.SiteKind.Let
            | root.algebra.SiteKind.CallArgument
            | root.algebra.SiteKind.Return
            | root.algebra.SiteKind.Field
            | root.algebra.SiteKind.ArrayElement =>
        {
            let outcome = if (compiles) {
                "holds, so the program compiles"
            } else {
                "does not hold, so the program is rejected"
            };
            `at ${root.algebra.site_phrase(site)}, a value of type ${s} meets a slot of type ${t}: ${s} <: ${t} ${outcome}`
        },
    };
    // The site's own rule is always what the answer turns on: it is the one
    // that says what the construct makes of the pair.
    root.engine.Claim { rule: site_rule(site), instance, bearing: true }
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
    // A pair that mentions a type variable can only be spelled where the
    // probe's own function binds it, so the site is drawn from what this
    // pair can actually be written at.
    let usable = root.algebra.sites_for(sites, root.algebra.left_of(d), root.algebra.right_of(d));
    if (usable.length() == 0) {
        return null;
    }
    let site_draw = root.engine.below(direction.next, usable.length());
    let reserved = root.algebra.decls_of_all([root.algebra.left_of(d), root.algebra.right_of(d)]).map((decl) -> {
        root.algebra.decl_name(decl)
    });
    Drawn {
        d,
        relation,
        reversed: direction.value == 1,
        site: usable[site_draw.value],
        names: site_names(site_draw.next, reserved),
    }
}

/// One direction of a fact at a site: the program it renders, what the
/// compiler must say about it, and why.
///
/// The site travels with it. What a site does with a relation is the site's
/// own question — a flow site accepts what flows into it, and the coherence
/// site rejects exactly the pair that is equivalent — so anything asking what
/// a belief about these types would predict has to ask that question, and
/// cannot ask it without the site. Carrying it here is what stops a reader
/// of this value reaching for the flow-site answer at a site that inverts it.
class Flowed {
    flow: Oriented,
    site: root.algebra.SiteKind,
    case: root.engine.Case,
    compiles: bool,
}

/// Whether a fact drawn at this site renders two programs that answer
/// differently, so it can be put side by side.
///
/// At a directed site an equivalence and an unrelated pair read the same
/// both ways, so only a strict relation pairs. At an undirected site nothing
/// does: the reverse draw is the same program with its parts the other way
/// up, and the same answer.
function pairs(site: root.algebra.SiteKind, relation: root.algebra.Relation) -> bool throws never {
    root.algebra.directed(site)
        && root.algebra.compiles_when(site, relation)
            != root.algebra.compiles_when(site, root.algebra.mirror(relation))
}

/// The pair of `drawn` flowing one way at its site, as a case.
function flowed(drawn: Drawn, reversed: bool) -> Flowed throws never {
    let flow = orient(drawn.d, drawn.relation, reversed);
    let probe = root.algebra.probe(drawn.site, flow.source, flow.target, drawn.names);
    let compiles = root.algebra.compiles_when(drawn.site, flow.relation);
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
    let trace = root.algebra.claims(drawn.d).concat([site_claim(drawn.site, flow, compiles)]);
    Flowed {
        flow,
        site: drawn.site,
        case: root.engine.Case { files: root.engine.one_file(file, probe.source), key, trace },
        compiles,
    }
}

/// What a model's beliefs predict the compiler will say about `one`.
///
/// The model's belief about the pair, read at the site the case stands at.
/// Null when the model's overrides leave the pair undecided, which is not a
/// prediction and so cannot be wrong.
function predicted(
    d: root.algebra.Derivation,
    one: Flowed,
    model: root.algebra.Model,
) -> bool? throws never {
    let believed = root.algebra.decided(root.algebra.under(d, model))
        ?? return null;
    root.algebra.compiles_when(one.site, in_direction(believed, one.flow))
}

/// The naive models that read one program's verdict the other way.
function flow_traps(d: root.algebra.Derivation, one: Flowed) -> string[] throws never {
    models().filter_map((model) -> {
        let says = predicted(d, one, model)
            ?? return null;
        if (says != one.compiles) {
            model.id
        } else {
            null
        }
    })
}

/// The naive models that either direction of `drawn` would fool.
///
/// A model's belief about the pair does not depend on which way it flows, so
/// it is replayed once and read both ways; nothing is rendered.
function pair_lures(drawn: Drawn) -> string[] throws never {
    let castings = [orient(drawn.d, drawn.relation, false), orient(drawn.d, drawn.relation, true)];
    models().filter_map((model) -> {
        let believed = root.algebra.decided(root.algebra.under(drawn.d, model))
            ?? return null;
        let fooled = castings.some((flow) -> {
            root.algebra.compiles_when(drawn.site, in_direction(believed, flow))
                != root.algebra.compiles_when(drawn.site, flow.relation)
        });
        if (fooled) {
            model.id
        } else {
            null
        }
    })
}

/// `drawn` flowing one way, as a candidate. Its features are the pair's and
/// its traps the case's, so the two directions differ in nothing a sampler
/// scores by.
function candidate_of(drawn: Drawn, reversed: bool) -> root.engine.Candidate throws never {
    let chosen = flowed(drawn, reversed);
    let lures = pair_lures(drawn);
    root.engine.Candidate {
        case: chosen.case,
        features: features_of(
            chosen.case.trace,
            root.algebra.flips(drawn.d),
            [chosen.flow.source, chosen.flow.target],
            lures,
        ),
        traps: flow_traps(drawn.d, chosen),
        lures,
    }
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
        let says_one = predicted(d, one, model)
            ?? return null;
        let says_other = predicted(d, other, model)
            ?? return null;
        if (says_one == says_other || says_one == one.compiles) {
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
            candidate_of(drawn, drawn.reversed)
        }

        function foil(self, seed: root.engine.Seed) -> root.engine.Foil? throws never {
            let drawn = draw_flow(self.fact, self.sites, seed)
                ?? return null;
            // Exactly one direction of a strict relation compiles, so a
            // strict fact, and only a strict fact, has a second program to
            // offer. That the two do answer differently is held to by the
            // suite, which reads it off the two keys.
            if (!pairs(drawn.site, drawn.relation)) {
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
            // The alias's own name, which this source always spells: byte
            // zero would be a region pointing at whatever happens to be
            // first, and a wrong region reads as a compiler disagreement.
            let at = source.index_of(a)
                ?? throw baml.panics.Unreachable { message: `no \`${a}\` in the case it names` };
            let prefix = source.slice(0, at);
            let start = prefix.to_utf8().length();
            let trace = [
                root.engine.Claim {
                    rule: unguarded_cycle_rejected(),
                    instance: `\`${a}\` is a fully unguarded cycle: it denotes \`never\` and is rejected`,
                    bearing: true,
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
                lures: [],
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

/// `let x: A | (B | C) = v` for a `v` of type `A | B | D`, which the spec
/// rejects — `D` is in neither part — and the compiler accepts: see
/// COMPILER_DIVERGENCE.md, CD-003. It is `union_regrouped_differs` put where
/// `sites_for` will not put it: at the binding, flowing into the side that
/// nests.
class NestedUnionLetItem {
    implements root.engine.Item {
        function id(self) -> string throws never {
            "unions/nested-in-let-annotation"
        }

        function status(self) -> root.engine.Status throws never {
            root.engine.CompilerDiverges {
                divergence: "CD-003",
                contradiction: root.engine.Contradiction.Accepts,
            }
        }

        function generate(self, seed: root.engine.Seed) -> root.engine.Candidate? throws never {
            let streams = root.engine.split(seed);
            let d = union_regrouped_differs(streams.left).value;
            let relation = root.algebra.decided(root.algebra.knowledge(d))
                ?? return null;
            let none: string[] = [];
            // The fact nests its left side, so flowing backwards is what
            // makes the nested union the annotation.
            let drawn = Drawn {
                d,
                relation,
                reversed: true,
                site: root.algebra.SiteKind.Let,
                names: site_names(streams.right, none),
            };
            candidate_of(drawn, true)
        }

        /// The other direction is rejected by the spec and the compiler
        /// alike, so there is no second program that answers differently.
        function foil(self, seed: root.engine.Seed) -> root.engine.Foil? throws never {
            null
        }
    }
}
