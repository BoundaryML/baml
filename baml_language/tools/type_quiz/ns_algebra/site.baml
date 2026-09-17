// Where a relation is exercised: the program a pair of types is put in, and
// the region of it the compiler must point inside when it objects. What
// follows from the relation is the site's own business — the flow sites
// accept exactly a subtype, while two `implements` blocks are rejected
// exactly when their targets are the same type — so `compiles_when` is what
// a generator asks, never the relation directly. Which diagnostic a rejected
// site produces is compiler surface and lives in the bank; where the compiler
// points is fixed here and held against it at every site by the suite.

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
    /// Two `implements` blocks for one interface, one for each type. Not a
    /// flow: nothing is assigned to anything, and what the compiler objects
    /// to is the pair being one type twice.
    Coherence,
}

/// Whether a pair standing in `relation` compiles at `site`.
///
/// A flow site takes exactly a subtype. Two `implements` blocks overlap
/// exactly when their targets are the same type, and overlapping is what
/// coherence forbids, so there the verdict is the other way up.
function compiles_when(site: SiteKind, relation: Relation) -> bool throws never {
    match (site) {
        SiteKind.Let
            | SiteKind.CallArgument
            | SiteKind.Return
            | SiteKind.Field
            | SiteKind.ArrayElement =>
        {
            flows(relation)
        },
        SiteKind.Coherence => relation != Relation.Equiv,
    }
}

/// Whether `site` puts its pair in one order rather than in two roles, so
/// that swapping the pair gives the same program back.
///
/// A flow has a direction: which type is the value and which the slot is the
/// question. Two `implements` blocks are symmetric, so the reverse draw is
/// the same program with its blocks the other way up and the same answer.
function directed(site: SiteKind) -> bool throws never {
    match (site) {
        SiteKind.Let
            | SiteKind.CallArgument
            | SiteKind.Return
            | SiteKind.Field
            | SiteKind.ArrayElement =>
        {
            true
        },
        SiteKind.Coherence => false,
    }
}

/// The identifiers a probe is spelled with. Cosmetic, supplied by the bank so
/// a learner cannot recognise a site by its names.
class SiteNames {
    function_name: string,
    value: string,
    target: string,
    sink: string,
    /// The binding that holds the constructed class at the `Field` site.
    instance: string,
    holder: string,
    /// The interface the two blocks implement at the `Coherence` site.
    contract: string,
}

/// A rendered probe and the byte region of it that a rejection must point
/// inside: the flowing expression, or at the `Return` site the function body
/// the compiler reports as a whole.
class Probe {
    source: string,
    start: int,
    end: int,
}

/// `head`, `expr`, and `tail` in sequence, with the region of `expr`.
function around(head: string, expr: string, tail: string) -> Probe throws never {
    let start = head.to_utf8().length();
    Probe { source: `${head}${expr}${tail}`, start, end: start + expr.to_utf8().length() }
}

/// Whether the target's spelling sits inside the probe's own function, where
/// a type variable in it is the same variable the flowing value is typed by.
/// At the other sites the target is declared elsewhere — a call's parameter,
/// a class's field — so a variable there would be a second, unrelated one.
function binds_target(site: SiteKind) -> bool throws never {
    match (site) {
        SiteKind.Let => true,
        SiteKind.Return => true,
        SiteKind.ArrayElement => true,
        SiteKind.CallArgument => false,
        SiteKind.Field => false,
        // Neither block is inside a function, so there is nothing to bind a
        // variable; `sites_for` keeps a variable away from here regardless,
        // since a variable is not an implementation target.
        SiteKind.Coherence => false,
    }
}

/// The sites of `sites` that can carry this pair.
///
/// Both types are consulted, not just the target, because one draw is
/// rendered in both directions at one site: a case's foil is its reverse, so
/// a site that cannot take the pair one way must not be drawn for the other.
function sites_for(sites: SiteKind[], source: Ty, target: Ty) -> SiteKind[] throws never {
    let bindable = if (!mentions_var(source) && !mentions_var(target)) {
        sites
    } else {
        sites.filter((site) -> {
            binds_target(site)
        })
    };
    // A pair spelled the same way makes two `implements` blocks that are
    // character for character identical, which a learner can answer by
    // spotting the duplication without knowing what coherence is — a model
    // that sat the quiz said exactly that. Keeping them anyway is MEASURED,
    // not conceded: holding them out costs certification badly (a perfect
    // learner certified in 6 of 12 sittings rather than 10, six sessions
    // flipping and none improving), because they are also the cheapest
    // evidence the rule has. The complaint was really that one was asked six
    // times over, which is selection's to fix, not the bank's.
    if (is_target(source) && is_target(target)) {
        bindable
    } else {
        bindable.filter((site) -> {
            site != SiteKind.Coherence
        })
    }
}

/// The probe for `site`, with the declarations `source` and `target` need.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if `target` mentions a type variable at a site
///   that declares the target outside the probe's function. `sites_for` is
///   what keeps a generator from asking.
function probe(site: SiteKind, source: Ty, target: Ty, names: SiteNames) -> Probe throws never {
    if (mentions_var(target) && !binds_target(site)) {
        throw baml.panics.Unreachable {
            message: `${render(target)} mentions a type variable, and ${site_phrase(site)} declares it outside the probe`,
        }
    }
    if (site == SiteKind.Coherence && (!is_target(source) || !is_target(target))) {
        throw baml.panics.Unreachable {
            message: `${render(source)} and ${render(target)} cannot both name an implementation target`,
        }
    }
    let decls = render_decls(decls_of_all([source, target]));
    // The probe's own function binds every variable the pair mentions, so
    // both occurrences of one name are one variable.
    let binders = render_binders(vars_of([source, target]));
    let s = render(source);
    let t = render(target);
    // Spelled through `render` so a union or function target keeps its
    // parentheses inside the element position.
    let t_array = render(Array.new(target));
    let v = names.value;
    match (site) {
        SiteKind.Let => {
            around(
                `${decls}function ${names.function_name}${binders}(${v}: ${s}) -> void {\n    let ${names.target}: ${t} = `,
                v,
                `;\n}\n`,
            )
        },
        SiteKind.CallArgument => {
            around(
                `${decls}function ${names.sink}(${names.target}: ${t}) -> void {\n}\n\nfunction ${names.function_name}${binders}(${v}: ${s}) -> void {\n    ${names.sink}(`,
                v,
                `);\n}\n`,
            )
        },
        SiteKind.Return => {
            around(
                `${decls}function ${names.function_name}${binders}(${v}: ${s}) -> ${t} `,
                `{\n    ${v}\n}`,
                `\n`,
            )
        },
        SiteKind.Field => {
            around(
                `${decls}class ${names.holder} {\n    ${names.target}: ${t},\n}\n\nfunction ${names.function_name}${binders}(${v}: ${s}) -> void {\n    let ${names.instance} = ${names.holder} { ${names.target}: `,
                v,
                ` };\n}\n`,
            )
        },
        SiteKind.ArrayElement => {
            around(
                `${decls}function ${names.function_name}${binders}(${v}: ${s}) -> void {\n    let ${names.target}: ${t_array} = [`,
                v,
                `];\n}\n`,
            )
        },
        SiteKind.Coherence => {
            // The interface is spelled through the renderer like any other
            // declaration, so a change in how an interface is written shows
            // up here too. It asks for nothing: what the case is about is
            // which types an implementation may be written for, never what
            // the interface wants of them.
            let contract = render_decl(IfaceDecl { name: names.contract, params: [], assoc: [], bounds: [] });
            around(
                `${decls}${contract}\n\nimplement ${names.contract} for ${s} {}\n\n`,
                `implement ${names.contract} for ${t} {}`,
                `\n`,
            )
        },
    }
}

/// Every site, for sweeps.
function all_sites() -> SiteKind[] throws never {
    [
        SiteKind.Let,
        SiteKind.CallArgument,
        SiteKind.Return,
        SiteKind.Field,
        SiteKind.ArrayElement,
        SiteKind.Coherence,
    ]
}

/// The site in prose, for a case's explanation.
function site_phrase(site: SiteKind) -> string throws never {
    match (site) {
        SiteKind.Let => "the initializer of an annotated binding",
        SiteKind.CallArgument => "a call argument",
        SiteKind.Return => "a function's return value",
        SiteKind.Field => "a field initializer",
        SiteKind.ArrayElement => "an element of an annotated array",
        SiteKind.Coherence => "two `implements` blocks for one interface",
    }
}
