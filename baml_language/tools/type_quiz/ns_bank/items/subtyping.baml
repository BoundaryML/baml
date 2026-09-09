// Items for the Subtyping Rules section.

/// A union does not fit back into one of its members: `int | string` is a
/// superset of `int`, so a binding typed `int` cannot accept it without
/// narrowing. The union binding only ever held an `int`, which is the
/// flow-sensitive trap.
class UnionNotAssignableToMember {
    implements root.engine.Item {
        function id(self) -> string throws never {
            "subtyping/union-not-assignable-to-member"
        }

        function status(self) -> root.engine.Status throws never {
            root.engine.Verified {  }
        }

        function generate(self, seed: root.engine.Seed) -> root.engine.Candidate? throws never {
            let fns = root.bank.pick_distinct(seed, root.bank.function_names(), 1);
            let vars = root.bank.pick_distinct(fns.next, root.bank.binding_names(), 3);
            let literal = root.engine.below(vars.next, 100);
            let f = fns.names[0];
            let a = vars.names[0];
            let b = vars.names[1];
            let c = vars.names[2];
            let source = `
                function ${f}() -> void {
                    let ${a}: int = ${literal.value};
                    let ${b}: int | string = ${a};
                    let ${c}: int = ${b};
                }
            `;
            let features: map<string, float> = {};
            root.engine.Candidate {
                case: root.engine.Case {
                    files: { "case.baml": source },
                    key: root.engine.Rejected { codes: ["E0001"], anchor: c },
                    trace: [root.bank.union_is_superset_of_member()],
                },
                features: features,
                traps: [],
            }
        }
    }
}
