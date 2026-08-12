//! `SymbolId` round-trips and human-path resolution.

use std::str::FromStr;

use baml_project::ProjectDatabase;

use crate::{Member, Resolved, Symbol, SymbolId, ids, resolve};

fn make_db() -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    db
}

const FIXTURE: &str = r#"
class Point {
  x int
}

enum Color { Red }

interface Encoder {
  type Error
  function encode(self, value: string) -> string throws Self.Error
}

function greet(name: string) -> string { name }
"#;

const DEFAULTED_FIXTURE: &str = r#"
interface Greeter {
  function greet(self) -> string
  function shout(self) -> string { self.greet() }
}

class Widget {
  name string

  implements Greeter {
    function greet(self) -> string { self.name }
  }
}
"#;

/// A class implementing one interface at two instantiations. This is the shape
/// that broke: both blocks contribute a method named `scaled`, so addressing
/// either on the class alone names both.
const MULTI_IMPL_FIXTURE: &str = r#"
interface Scale<By extends baml.Concrete> requires baml.Concrete {
  function scaled(self, by: By) -> int
}

class Meters {
  value int

  implements Scale<int> {
    function scaled(self, by: int) -> int { self.value * by }
  }

  implements Scale<string> {
    function scaled(self, by: string) -> int { self.value * by.length() }
  }
}
"#;

/// Two instantiations of one interface contribute two distinct ids.
///
/// Before the impl-qualified form both were `M:user.Meters.scaled`, and the
/// export document carried the same id on two different methods — a consumer
/// keyed on it got a coin flip. Multi-RHS operator overloading is precisely
/// what a parameterized interface is for, so this is not an exotic case.
#[test]
fn one_interface_at_two_instantiations_yields_two_ids() {
    let mut db = make_db();
    db.add_file("multi.baml", MULTI_IMPL_FIXTURE);

    let Some(Resolved::Symbol(Symbol::Class(class))) = resolve(&db, "Meters") else {
        panic!("Meters resolves to a class")
    };
    let ids: Vec<String> = class
        .methods(&db)
        .into_iter()
        .filter(|f| f.name(&db).as_str() == "scaled")
        .filter_map(|f| SymbolId::of_symbol(&db, Symbol::Function(f)))
        .map(|id| id.to_string())
        .collect();

    assert_eq!(ids.len(), 2, "both blocks contribute a `scaled`: {ids:?}");
    assert!(
        ids.contains(&"M:(user.Meters as user.Scale<int>).scaled".to_string()),
        "the int instantiation is named by its argument: {ids:?}"
    );
    assert!(
        ids.contains(&"M:(user.Meters as user.Scale<string>).scaled".to_string()),
        "the string instantiation is named by its argument: {ids:?}"
    );

    // And each id finds its own method back, rather than whichever the name
    // lookup happened to reach first.
    for id in &ids {
        let parsed = SymbolId::from_str(id).expect("an emitted id parses");
        assert_eq!(&parsed.to_string(), id, "round-trips verbatim");
        let Some(Resolved::Member(Symbol::Impl(_), Member::Method(found))) = parsed.resolve(&db)
        else {
            panic!("{id} resolves to an impl method")
        };
        assert_eq!(found.name(&db).as_str(), "scaled");
        assert_eq!(
            SymbolId::of_symbol(&db, Symbol::Function(found)).map(|i| i.to_string()),
            Some(id.clone()),
            "resolution lands on the method the id names"
        );
    }
}

/// An inherited default is reachable through every block that inherits it, and
/// still names one declaration.
///
/// The two ids answer different questions and both are needed: the export
/// publishes a record per block, so the access path must resolve, while
/// `declared_by` carries the single place the code is written. Collapsing them
/// would either make a published record unaddressable or claim thirteen
/// implementors had thirteen `chain`s.
#[test]
fn an_inherited_default_is_reachable_through_the_block_and_declared_once() {
    let mut db = make_db();
    db.add_file("defaults.baml", DEFAULTED_FIXTURE);

    // The override the block writes itself.
    let overridden = SymbolId::from_str("M:(user.Widget as user.Greeter).greet")
        .expect("the override's id parses");
    assert!(
        overridden.resolve(&db).is_some(),
        "a method the block declares resolves through it"
    );

    // The default it inherits, reached through the same block.
    let inherited = SymbolId::from_str("M:(user.Widget as user.Greeter).shout")
        .expect("the access path parses");
    let Some(Resolved::Member(Symbol::Impl(_), Member::Method(shout))) = inherited.resolve(&db)
    else {
        panic!("an inherited default resolves through the block that inherits it")
    };

    // And it is declared exactly once, on the interface.
    assert_eq!(
        SymbolId::of_symbol(&db, Symbol::Function(shout)).map(|id| id.to_string()),
        Some("M:user.Greeter.shout".to_string()),
        "the declaration keeps the interface's id, however it was reached"
    );
}

/// The parenthesized form survives a for-type that is itself parenthesized,
/// and an interface argument carrying its own `as`.
#[test]
fn impl_owned_ids_parse_around_nested_projections() {
    let id = "M:((Self as baml.Comparable).CompareError as baml.ops.Equals<int>).eq";
    let parsed = SymbolId::from_str(id).expect("a nested projection parses");
    let ids::Owner::Impl { for_ty, interface } = &parsed.owner else {
        panic!("parsed as impl-owned")
    };
    assert_eq!(for_ty, "(Self as baml.Comparable).CompareError");
    assert_eq!(interface, "baml.ops.Equals<int>");
    assert_eq!(parsed.member.as_deref(), Some("eq"));
    assert_eq!(parsed.to_string(), id, "round-trips verbatim");

    // A function type's parens and arrow do not confuse the split either.
    let arrowed = "M:(((int) -> string) as baml.ToString).to_string";
    let parsed = SymbolId::from_str(arrowed).expect("a function for-type parses");
    let ids::Owner::Impl { for_ty, .. } = &parsed.owner else {
        panic!("parsed as impl-owned")
    };
    assert_eq!(for_ty, "((int) -> string)");
    assert_eq!(parsed.to_string(), arrowed);
}

/// Every id round-trips: symbol → id → string → id → the same symbol.
#[test]
fn symbol_ids_round_trip_through_strings_and_resolution() {
    let mut db = make_db();
    db.add_file("fixture.baml", FIXTURE);

    let cases = [
        ("Point", "T:user.Point"),
        ("greet", "V:user.greet"),
        ("baml.String", "T:baml.String"),
        ("baml.json.parse", "V:baml.json.parse"),
    ];
    for (path, expected_id) in cases {
        let Some(Resolved::Symbol(symbol)) = resolve(&db, path) else {
            panic!("{path} resolves to a symbol")
        };
        let id = SymbolId::of_symbol(&db, symbol).unwrap_or_else(|| panic!("{path} has an id"));
        assert_eq!(id.to_string(), expected_id, "{path}");

        let reparsed = SymbolId::from_str(&id.to_string()).unwrap();
        assert_eq!(reparsed, id, "{path} string round-trip");
        let Some(Resolved::Symbol(back)) = reparsed.resolve(&db) else {
            panic!("{expected_id} resolves back")
        };
        assert_eq!(back, symbol, "{path} db round-trip");
    }
}

#[test]
fn member_ids_round_trip() {
    let mut db = make_db();
    db.add_file("fixture.baml", FIXTURE);

    let cases = [
        ("baml.time.Duration.abs", "M:baml.time.Duration.abs"),
        ("baml.String.split", "M:baml.String.split"),
        ("Point.x", "F:user.Point.x"),
        ("Color.Red", "E:user.Color.Red"),
        ("Encoder.Error", "A:user.Encoder.Error"),
        ("Encoder.encode", "M:user.Encoder.encode"),
        ("baml.Comparable.compare", "M:baml.Comparable.compare"),
    ];
    for (path, expected_id) in cases {
        let Some(Resolved::Member(owner, member)) = resolve(&db, path) else {
            panic!("{path} resolves to a member")
        };
        let id = SymbolId::of_member(&db, owner, member).unwrap();
        assert_eq!(id.to_string(), expected_id, "{path}");

        let reparsed = SymbolId::from_str(expected_id).unwrap();
        let Some(Resolved::Member(_, back)) = reparsed.resolve(&db) else {
            panic!("{expected_id} resolves back")
        };
        assert_eq!(back, member, "{path} db round-trip");
    }
}

/// A class method's *symbol* id nests under its owning type.
#[test]
fn method_symbol_ids_nest_under_their_type() {
    let db = make_db();
    let Some(Resolved::Member(_, Member::Method(split))) = resolve(&db, "baml.String.split") else {
        panic!("String.split is a method")
    };
    let id = SymbolId::of_symbol(&db, Symbol::Function(split)).unwrap();
    assert_eq!(id.to_string(), "M:baml.String.split");
}

#[test]
fn human_path_routing() {
    let mut db = make_db();
    db.add_file("fixture.baml", FIXTURE);

    // Packages and namespaces resolve as themselves.
    assert!(matches!(resolve(&db, "baml"), Some(Resolved::Package(_))));
    assert!(matches!(
        resolve(&db, "baml.json"),
        Some(Resolved::Namespace(_))
    ));
    // `root.` forces the user package.
    assert!(matches!(
        resolve(&db, "root.Point"),
        Some(Resolved::Symbol(Symbol::Class(_)))
    ));
    // Unqualified builtin fallback.
    assert!(matches!(
        resolve(&db, "String"),
        Some(Resolved::Symbol(Symbol::Class(_)))
    ));
    assert!(matches!(
        resolve(&db, "json.parse"),
        Some(Resolved::Symbol(Symbol::Function(_)))
    ));
    // Misses stay misses.
    assert_eq!(resolve(&db, "NoSuchThing"), None);
    assert_eq!(resolve(&db, "root.String"), None);
    assert_eq!(resolve(&db, "baml..String"), None);
}

#[test]
fn id_strings_reject_malformed_input() {
    for bad in [
        "baml.String",   // no kind prefix
        "Q:baml.String", // unknown prefix
        "T:String",      // no package segment
        "M:baml.split",  // member kind without pkg.Type.member shape
        "T:baml..String",
        "M:baml.String.",
    ] {
        assert!(SymbolId::from_str(bad).is_err(), "{bad} must not parse");
    }
    // serde round-trip.
    let id = ids::SymbolId::from_str("M:baml.String.split").unwrap();
    let json = serde_json::to_string(&id).unwrap();
    let back: ids::SymbolId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, id);
}
