// Where a relation is exercised: the probe program in which a value of the
// source type flows into a slot of the target type. Every site here is a
// covariant flow, so the probe compiles exactly when the source is a subtype
// of (or the same type as) the target. Which diagnostic a rejected flow
// produces at each site is compiler surface and lives in the bank.

enum SiteKind {
    /// `let target: R = value;`
    Let,
    /// `sink(value)` where `sink` takes `R`.
    CallArgument,
    /// `value` as the body of a function returning `R`.
    Return,
    /// `Holder { target: value }` where the field has type `R`.
    Field,
    /// `let target: R[] = [value];`
    ArrayElement,
}

/// The identifiers a probe is spelled with. Cosmetic, supplied by the bank so
/// a learner cannot recognise a site by its names.
class SiteNames {
    function_name: string,
    value: string,
    target: string,
    sink: string,
    holder: string,
}

/// A rendered probe: the source and the name on the line a rejection
/// should point at.
class Probe {
    source: string,
    anchor: string,
}

/// The probe for `site`, with the declarations `source` and `target` need.
function probe(site: SiteKind, source: Ty, target: Ty, names: SiteNames) -> Probe throws never {
    let decls = render_decls(decls_of_all([source, target]));
    let s = render(source);
    let t = render(target);
    // Spelled through `render` so a union or function target keeps its
    // parentheses inside the element position.
    let t_array = render(array(target));
    match (site) {
        SiteKind.Let => {
            Probe {
                source: `
                    ${decls}function ${names.function_name}(${names.value}: ${s}) -> void {
                        let ${names.target}: ${t} = ${names.value};
                    }
                `,
                anchor: names.target,
            }
        },
        SiteKind.CallArgument => {
            Probe {
                source: `
                    ${decls}function ${names.sink}(${names.target}: ${t}) -> void {
                    }

                    function ${names.function_name}(${names.value}: ${s}) -> void {
                        ${names.sink}(${names.value});
                    }
                `,
                anchor: names.sink,
            }
        },
        SiteKind.Return => {
            Probe {
                source: `
                    ${decls}function ${names.function_name}(${names.value}: ${s}) -> ${t} {
                        ${names.value}
                    }
                `,
                anchor: names.function_name,
            }
        },
        SiteKind.Field => {
            Probe {
                source: `
                    ${decls}class ${names.holder} {
                        ${names.target}: ${t},
                    }

                    function ${names.function_name}(${names.value}: ${s}) -> void {
                        let ${names.sink} = ${names.holder} { ${names.target}: ${names.value} };
                    }
                `,
                anchor: names.target,
            }
        },
        SiteKind.ArrayElement => {
            Probe {
                source: `
                    ${decls}function ${names.function_name}(${names.value}: ${s}) -> void {
                        let ${names.target}: ${t_array} = [${names.value}];
                    }
                `,
                anchor: names.target,
            }
        },
    }
}

/// Every site, for sweeps.
function all_sites() -> SiteKind[] throws never {
    [SiteKind.Let, SiteKind.CallArgument, SiteKind.Return, SiteKind.Field, SiteKind.ArrayElement]
}
