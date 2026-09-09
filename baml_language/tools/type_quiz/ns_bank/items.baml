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
            let streams = root.engine.split(seed);
            let d = self.fact.generate(streams.left).value;
            let relation = root.algebra.decided(root.algebra.knowledge(d))
                ?? return null;
            let direction = root.engine.below(streams.right, 2);
            let flow = orient(d, relation, direction.value == 1);
            let site_draw = root.engine.below(direction.next, self.sites.length());
            let site = self.sites[site_draw.value];
            let reserved = root.algebra.decls_of_all([flow.source, flow.target]).map((decl) -> {
                root.algebra.decl_name(decl)
            });
            let names = site_names(site_draw.next, reserved);
            let probe = root.algebra.probe(site, flow.source, flow.target, names);
            let compiles = root.algebra.flows(flow.relation);
            let file = "case.baml";
            let key: root.engine.Verdict = if (compiles) {
                root.engine.Compiles { tests: [] }
            } else {
                root.engine.Rejected {
                    code: rejection_code(site),
                    more: [],
                    at: root.engine.Region { file, start: probe.start, end: probe.end },
                }
            };
            let traps = models().filter_map((model) -> {
                let believed = root.algebra.decided(root.algebra.under(d, model))
                    ?? return null;
                if (root.algebra.flows(in_direction(believed, flow)) != compiles) {
                    model.id
                } else {
                    null
                }
            });
            let trace = root.algebra.claims(d).concat([flow_claim(site, flow, compiles)]);
            root.engine.Candidate {
                case: root.engine.Case { files: root.engine.one_file(file, probe.source), key, trace },
                features: features_of(trace, root.algebra.flips(d), [flow.source, flow.target], traps),
                traps,
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
    }
}
