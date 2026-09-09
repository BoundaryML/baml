// Where a relation is exercised: the probe program in which a value of the
// source type flows into a slot of the target type. Every site here is a
// covariant flow, so the probe compiles exactly when the source is a subtype
// of (or the same type as) the target. Which diagnostic a rejected flow
// produces at each site is compiler surface and lives in the bank; where
// the compiler points is fixed here, as a byte region of the probe, and the
// conformance suite holds it against the compiler at every site.

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
    /// The binding that holds the constructed class at the `Field` site.
    instance: string,
    holder: string,
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

/// The probe for `site`, with the declarations `source` and `target` need.
function probe(site: SiteKind, source: Ty, target: Ty, names: SiteNames) -> Probe throws never {
    let decls = render_decls(decls_of_all([source, target]));
    let s = render(source);
    let t = render(target);
    // Spelled through `render` so a union or function target keeps its
    // parentheses inside the element position.
    let t_array = render(Array.new(target));
    let v = names.value;
    match (site) {
        SiteKind.Let => {
            around(
                `${decls}function ${names.function_name}(${v}: ${s}) -> void {\n    let ${names.target}: ${t} = `,
                v,
                `;\n}\n`,
            )
        },
        SiteKind.CallArgument => {
            around(
                `${decls}function ${names.sink}(${names.target}: ${t}) -> void {\n}\n\nfunction ${names.function_name}(${v}: ${s}) -> void {\n    ${names.sink}(`,
                v,
                `);\n}\n`,
            )
        },
        SiteKind.Return => {
            around(`${decls}function ${names.function_name}(${v}: ${s}) -> ${t} `, `{\n    ${v}\n}`, `\n`)
        },
        SiteKind.Field => {
            around(
                `${decls}class ${names.holder} {\n    ${names.target}: ${t},\n}\n\nfunction ${names.function_name}(${v}: ${s}) -> void {\n    let ${names.instance} = ${names.holder} { ${names.target}: `,
                v,
                ` };\n}\n`,
            )
        },
        SiteKind.ArrayElement => {
            around(
                `${decls}function ${names.function_name}(${v}: ${s}) -> void {\n    let ${names.target}: ${t_array} = [`,
                v,
                `];\n}\n`,
            )
        },
    }
}

/// Every site, for sweeps.
function all_sites() -> SiteKind[] throws never {
    [SiteKind.Let, SiteKind.CallArgument, SiteKind.Return, SiteKind.Field, SiteKind.ArrayElement]
}

/// The site in prose, for a case's explanation.
function site_phrase(site: SiteKind) -> string throws never {
    match (site) {
        SiteKind.Let => "the initializer of an annotated binding",
        SiteKind.CallArgument => "a call argument",
        SiteKind.Return => "a function's return value",
        SiteKind.Field => "a field initializer",
        SiteKind.ArrayElement => "an element of an annotated array",
    }
}
