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
