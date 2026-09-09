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

/// A fact exercised at a flow site. Half the seeds flow the pair forwards
/// and half backwards, so a strict subtype fact yields a compiling case as
/// often as a rejected one and the shape never gives the verdict away.
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
            let streams = root.engine.split(seed);
            let d = self.fact.generate(streams.left).value;
            match (root.algebra.knowledge(d)) {
                root.algebra.Undecided => null,
                let relation: root.algebra.Relation => {
                    let direction = root.engine.below(streams.right, 2);
                    let reverse = direction.value == 1;
                    let source = if (reverse) {
                        d.right
                    } else {
                        d.left
                    };
                    let target = if (reverse) {
                        d.left
                    } else {
                        d.right
                    };
                    let flowing = if (reverse) {
                        root.algebra.mirror(relation)
                    } else {
                        relation
                    };
                    let site_draw = root.engine.below(direction.next, self.sites.length());
                    let site = self.sites[site_draw.value];
                    let reserved = root.algebra.decls_of_all([source, target]).map((decl) -> {
                        root.algebra.decl_name(decl)
                    });
                    let names = site_names(site_draw.next, reserved);
                    let probe = root.algebra.probe(site, source, target, names);
                    let compiles = root.algebra.flows(flowing);
                    let key: root.engine.Verdict = if (compiles) {
                        root.engine.Compiles { tests: [] }
                    } else {
                        root.engine.Rejected { codes: [rejection_code(site)], anchor: probe.anchor }
                    };
                    let traps = models().filter_map((model) -> {
                        match (root.algebra.under(d, model)) {
                            root.algebra.Undecided => null,
                            let believed: root.algebra.Relation => {
                                let flowing_under = if (reverse) {
                                    root.algebra.mirror(believed)
                                } else {
                                    believed
                                };
                                if (root.algebra.flows(flowing_under) != compiles) {
                                    model.id
                                } else {
                                    null
                                }
                            },
                        }
                    });
                    root.engine.Candidate {
                        case: root.engine.Case {
                            files: { "case.baml": probe.source },
                            key: key,
                            trace: root.algebra.steps(d)
                                .map((step) -> {
                                    root.algebra.step_rule(step)
                                })
                                .concat([flow_requires_subtype()]),
                        },
                        features: features_of(d, source, target, traps),
                        traps: traps,
                    }
                },
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
            root.engine.CompilerDiverges { divergence: "CD-001" }
        }

        function generate(self, seed: root.engine.Seed) -> root.engine.Candidate? throws never {
            let names = root.engine.pick_distinct(seed, type_names(), 2);
            let form = root.engine.below(names.next, 2);
            let a = names.value[0];
            let b = names.value[1];
            let source = if (form.value == 0) {
                `
                    type ${a} = ${a};
                `
            } else {
                `
                    type ${a} = ${b};
                    type ${b} = ${a};
                `
            };
            let features: map<string, float> = {};
            root.engine.Candidate {
                case: root.engine.Case {
                    files: { "case.baml": source },
                    key: root.engine.Rejected { codes: ["E0068"], anchor: a },
                    trace: [unguarded_cycle_rejected()],
                },
                features: features,
                traps: [],
            }
        }
    }
}
