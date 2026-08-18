//! Handle-layer tests: the syntactic surface over a fixture project, and
//! light probes over the real `baml` builtin package.

use std::fmt::Write as _;

use baml_project::ProjectDatabase;

use crate::{Db, FunctionOwner, Package, Symbol};

/// Bind insta's snapshot directory for FILE-based snapshots in relocated
/// builds.
///
/// insta resolves a file snapshot against the compile-time
/// `CARGO_MANIFEST_DIR`, and the CI nix unit graph compiles this crate in a
/// build sandbox (`/build/baml_surface-<ver>/`) that does not exist when the
/// prebuilt test binary later runs on a runner - insta then reads every
/// assertion as "+new results" and fails. `BAML_SURFACE_SNAPSHOT_DIR` lets
/// such a runner point insta at the real checkout's `src/snapshots`; unset
/// or empty (every local and cargo-arm run), behavior is byte-identical to a
/// bare `assert_snapshot!`. The override must be an ABSOLUTE path: insta
/// resolves a relative one against the assertion's source file, silently
/// selecting a wrong directory. Same pattern as `BAML_PARAM_SCHEMA_GOLDEN`
/// in `baml_project`, which exists for the same relocated-build reason.
pub(crate) fn with_snapshot_dir(assertion: impl FnOnce()) {
    let mut settings = insta::Settings::clone_current();
    if let Some(dir) = std::env::var_os("BAML_SURFACE_SNAPSHOT_DIR")
        && !dir.is_empty()
    {
        let dir = std::path::PathBuf::from(dir);
        assert!(
            dir.is_absolute(),
            "BAML_SURFACE_SNAPSHOT_DIR must be an absolute path (insta resolves \
             a relative one against the assertion source file), got {}",
            dir.display()
        );
        settings.set_snapshot_path(dir);
    }
    settings.bind(assertion);
}

fn make_db() -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(std::path::Path::new("."));
    db
}

const FIXTURE: &str = r##"/// Widget docs.
class Widget {
  name string

  function describe(self) -> string { self.name }
}

/// Color docs.
enum Color { Red, Green }

/// Renderer docs.
interface Renderer {
  function render(self) -> string throws never

  function fallback(self) -> string throws never { "?" }
}

/// Impl docs.
implements Renderer for int {
  function render(self) -> string throws never { "int" }
}

/// Alias docs.
type Id = string

function greet(name: string) -> string { name }


client<llm> Fast {
  provider openai
  options { model "gpt-4o-mini" }
}

retry_policy Careful {
  max_retries 2
}

test greet_works {
  functions [greet]
  args {}
}
"##;

/// One line per namespace item: kind, source kind when it differs, name, the
/// name-span slice (proving spans line up), and the docstring's first line.
fn render_user_surface(db: &dyn Db, src: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for ns in Package::user(db).namespaces(db) {
        let path: Vec<String> = ns.path(db).iter().map(ToString::to_string).collect();
        let _ = writeln!(out, "namespace [{}]", path.join("."));
        for (name, symbol) in ns.items(db) {
            let kind = symbol.kind();
            let source_kind = symbol.source_kind(db);
            let kind_label = if source_kind == kind {
                kind.as_str().to_string()
            } else {
                format!("{} (written: {})", kind.as_str(), source_kind.as_str())
            };
            let name_slice = symbol
                .name_span(db)
                .map(|span| &src[usize::from(span.start())..usize::from(span.end())])
                .unwrap_or("<unnamed>");
            let doc = symbol
                .docstring(db)
                .and_then(|d| d.lines().next())
                .unwrap_or("-");
            let synthetic = if symbol.is_synthetic(db) {
                " [synthetic]"
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "  {kind_label:<24} {name:<12} span:{name_slice:<12} doc: {doc}{synthetic}"
            );
        }
    }
    out
}

#[test]
fn user_surface_lists_every_kind_with_spans_and_docs() {
    let mut db = make_db();
    let file = db.add_file("fixture.baml", FIXTURE);
    let _ = file;

    with_snapshot_dir(|| insta::assert_snapshot!(render_user_surface(&db, FIXTURE)));
}

#[test]
fn ownership_edges_connect_methods_to_their_containers() {
    let mut db = make_db();
    db.add_file("fixture.baml", FIXTURE);

    let root = *Package::user(&db)
        .namespaces(&db)
        .first()
        .expect("user root namespace");
    let items = root.items(&db);
    let find = |wanted: &str| {
        items
            .iter()
            .find(|(name, _)| name.as_str() == wanted)
            .map(|(_, symbol)| *symbol)
            .unwrap_or_else(|| panic!("missing item {wanted}"))
    };

    // Class method: owner edge points back at the class.
    let Symbol::Class(widget) = find("Widget") else {
        panic!("Widget is a class")
    };
    let methods = widget.methods(&db);
    assert_eq!(methods.len(), 1);
    assert_eq!(methods[0].name(&db).as_str(), "describe");
    assert_eq!(methods[0].owner(&db), Some(FunctionOwner::Class(widget)));

    // Interface default method: owned by the interface.
    let Symbol::Interface(renderer) = find("Renderer") else {
        panic!("Renderer is an interface")
    };
    let defaults = renderer.default_methods(&db);
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].name(&db).as_str(), "fallback");
    assert_eq!(
        defaults[0].owner(&db),
        Some(FunctionOwner::Interface(renderer))
    );

    // Free-impl method: owned by the (unnamed) impl, which carries the docs.
    let file = widget.file(&db);
    let free_impls: Vec<crate::Impl> = baml_compiler2_ppir::item_data::file_free_impls(&db, file)
        .iter()
        .map(|&loc| loc.into())
        .collect();
    assert_eq!(free_impls.len(), 1);
    let imp = free_impls[0];
    assert_eq!(imp.docstring(&db), Some("Impl docs."));
    let impl_methods = imp.methods(&db);
    assert_eq!(impl_methods.len(), 1);
    assert_eq!(impl_methods[0].name(&db).as_str(), "render");
    assert_eq!(impl_methods[0].owner(&db), Some(FunctionOwner::Impl(imp)));

    // Free function: no owner.
    let Symbol::Function(greet) = find("greet") else {
        panic!("greet is a function")
    };
    assert_eq!(greet.owner(&db), None);
}

#[test]
fn builtin_baml_package_is_reachable() {
    let db = make_db();
    let baml = Package::named(&db, "baml");
    let namespaces = baml.namespaces(&db);
    assert!(!namespaces.is_empty(), "baml package has namespaces");

    let root = namespaces
        .iter()
        .find(|ns| ns.path(&db).is_empty())
        .expect("baml root namespace");
    let items = root.items(&db);
    let find = |wanted: &str| {
        items
            .iter()
            .find(|(name, _)| name.as_str() == wanted)
            .map(|(_, symbol)| *symbol)
            .unwrap_or_else(|| panic!("missing baml.{wanted}"))
    };

    let Symbol::Class(string) = find("String") else {
        panic!("baml.String is a class")
    };
    assert!(
        string
            .docstring(&db)
            .is_some_and(|d| d.contains("UTF-8 encoded string")),
        "baml.String keeps its docstring"
    );
    assert!(
        string.methods(&db).len() > 30,
        "baml.String lists its methods"
    );

    let Symbol::Interface(comparable) = find("Comparable") else {
        panic!("baml.Comparable is an interface")
    };
    assert!(comparable.docstring(&db).is_some());
}

// ── Typed surface ────────────────────────────────────────────────────────────

const TYPED_FIXTURE: &str = r#"
class Point {
  /// Horizontal.
  x int
  y float?
}

type Pair = Point[]

enum Mode { Fast, Slow }

function risky(a: int) -> int throws baml.panics.DivisionByZero | baml.errors.Io {
  a
}

function wrapper(a: int) -> int { risky(a) }

function pick<T extends baml.Comparable>(items: T[]) -> T? throws never {
  items.at(0)
}
"#;

fn render_typed_surface(db: &dyn Db) -> String {
    let mut out = String::new();
    let root = *Package::user(db).namespaces(db).first().unwrap();
    for (name, symbol) in root.items(db) {
        if symbol.is_synthetic(db) {
            continue;
        }
        match symbol {
            Symbol::Function(f) => {
                let sig = f.signature(db);
                let params: Vec<String> = sig
                    .params
                    .iter()
                    .map(|p| {
                        format!(
                            "{}: {}",
                            p.name.as_ref().map_or("_", |n| n.as_str()),
                            p.ty.render_canonical()
                        )
                    })
                    .collect();
                let generics: Vec<String> = f
                    .generic_params(db)
                    .iter()
                    .map(|(param, bounds)| {
                        if bounds.is_empty() {
                            param.as_str().to_string()
                        } else {
                            let names: Vec<String> =
                                bounds.iter().map(|b| b.name.render_user_facing()).collect();
                            format!("{} extends {}", param.as_str(), names.join(" & "))
                        }
                    })
                    .collect();
                let generics = if generics.is_empty() {
                    String::new()
                } else {
                    format!("<{}>", generics.join(", "))
                };
                let _ = writeln!(
                    out,
                    "function {name}{generics}({}) -> {}",
                    params.join(", "),
                    sig.return_type.render_canonical()
                );
                let throws = f.throws(db);
                let _ = writeln!(
                    out,
                    "  declared: {}",
                    throws
                        .declared
                        .map_or("-".to_string(), baml_type::Ty::render_canonical)
                );
                let leaves = |tys: &[baml_type::Ty]| -> String {
                    let mut names: Vec<String> =
                        tys.iter().map(baml_type::Ty::render_canonical).collect();
                    names.sort();
                    if names.is_empty() {
                        "-".to_string()
                    } else {
                        names.join(", ")
                    }
                };
                let _ = writeln!(out, "  panics:   {}", leaves(&throws.panics));
                let _ = writeln!(out, "  errors:   {}", leaves(&throws.errors));
            }
            Symbol::Class(c) => {
                let _ = writeln!(out, "class {name}");
                for field in c.fields(db) {
                    let _ = writeln!(
                        out,
                        "  {}: {}  doc: {}",
                        field.name(db),
                        field.ty(db).render_canonical(),
                        field.docstring(db).unwrap_or("-")
                    );
                }
            }
            Symbol::TypeAlias(a) => {
                let _ = writeln!(out, "type {name} = {}", a.resolved(db).render_canonical());
            }
            Symbol::Enum(e) => {
                let variants: Vec<String> = e
                    .variants(db)
                    .iter()
                    .map(|v| v.name(db).to_string())
                    .collect();
                let _ = writeln!(out, "enum {name} {{ {} }}", variants.join(", "));
            }
            _ => {}
        }
    }
    out
}

#[test]
fn typed_surface_resolves_signatures_throws_and_fields() {
    let mut db = make_db();
    db.add_file("typed.baml", TYPED_FIXTURE);

    with_snapshot_dir(|| insta::assert_snapshot!(render_typed_surface(&db)));
}

#[test]
fn builtin_methods_resolve_through_handles() {
    let db = make_db();
    let baml = Package::named(&db, "baml");
    let namespaces = baml.namespaces(&db);
    let root = namespaces
        .iter()
        .find(|ns| ns.path(&db).is_empty())
        .expect("baml root namespace");
    let Some((_, Symbol::Class(string))) = root
        .items(&db)
        .into_iter()
        .find(|(name, _)| name.as_str() == "String")
    else {
        panic!("baml.String is a class")
    };

    let split = string
        .methods(&db)
        .into_iter()
        .find(|m| m.name(&db).as_str() == "split")
        .expect("String.split exists");
    let sig = split.signature(&db);
    assert_eq!(sig.return_type.render_canonical(), "string[]");
    let throws = split.throws(&db);
    assert!(throws.panics.is_empty());
    assert!(throws.errors.is_empty(), "{:?}", throws.errors);
}

// ── Impl attachment (rustdoc-style) ──────────────────────────────────────────

/// Render one impl as a rustdoc-style header + member list.
fn render_impl(db: &dyn Db, imp: crate::Impl) -> String {
    let generics = imp
        .generic_params(db)
        .unwrap_or_default()
        .iter()
        .map(|(param, bounds)| {
            if bounds.is_empty() {
                param.as_str().to_string()
            } else {
                let names: Vec<String> =
                    bounds.iter().map(|b| b.name.render_user_facing()).collect();
                format!("{} extends {}", param.as_str(), names.join(" & "))
            }
        })
        .collect::<Vec<_>>();
    let generics = if generics.is_empty() {
        String::new()
    } else {
        format!("<{}>", generics.join(", "))
    };
    let iface = imp
        .interface(db)
        .map(|i| i.qualified_name(db).render_dotted(false))
        .unwrap_or_else(|| "<unresolved>".to_string());
    let for_ty = imp
        .for_ty(db)
        .map(baml_type::Ty::render_canonical)
        .unwrap_or_default();
    let mut out = format!("implements{generics} {iface} for {for_ty}\n");
    for (name, ty) in imp.assoc_bindings(db).unwrap_or_default() {
        let _ = writeln!(out, "  type {name} = {}", ty.render_canonical());
    }
    let mut methods: Vec<String> = imp
        .all_methods(db)
        .iter()
        .map(|m| {
            format!(
                "  fn {}{}\n",
                m.function.name(db),
                if m.from_default { " [default]" } else { "" }
            )
        })
        .collect();
    methods.sort();
    for m in methods {
        out.push_str(&m);
    }
    out
}

#[test]
fn builtin_int_lists_its_free_impls() {
    let db = make_db();
    let namespaces = Package::named(&db, "baml").namespaces(&db);
    let root = namespaces
        .iter()
        .find(|ns| ns.path(&db).is_empty())
        .unwrap();
    let Some((_, Symbol::Class(int))) = root
        .items(&db)
        .into_iter()
        .find(|(name, _)| name.as_str() == "Int")
    else {
        panic!("baml.Int is a class")
    };

    // The original false-gap: `implements Comparable for int` lives in
    // comparable.baml, cross-file and primitive-headed — invisible to the
    // legacy describe path.
    let rendered: Vec<String> = int
        .trait_impls(&db)
        .into_iter()
        .map(|imp| render_impl(&db, imp))
        .collect();
    let all = rendered.join("");
    assert!(
        all.contains("implements baml.Comparable for int") && all.contains("fn compare"),
        "baml.Int lists Comparable::compare, got:\n{all}"
    );
}

#[test]
fn builtin_array_lists_generic_impls_with_bindings() {
    let db = make_db();
    let namespaces = Package::named(&db, "baml").namespaces(&db);
    let root = namespaces
        .iter()
        .find(|ns| ns.path(&db).is_empty())
        .unwrap();
    let Some((_, Symbol::Class(array))) = root
        .items(&db)
        .into_iter()
        .find(|(name, _)| name.as_str() == "Array")
    else {
        panic!("baml.Array is a class")
    };

    let all: String = array
        .trait_impls(&db)
        .into_iter()
        .map(|imp| render_impl(&db, imp))
        .collect();
    // `implements<T extends Comparable> Sortable for T[]` — a generic impl a
    // bound-discharging lookup would silently drop.
    assert!(
        all.contains("implements<T extends baml.Comparable> baml.Sortable for T[]"),
        "generic Sortable impl attaches to baml.Array, got:\n{all}"
    );
    assert!(
        all.contains("type SortError = (T as baml.Comparable).CompareError"),
        "assoc binding resolves symbolically, got:\n{all}"
    );
    assert!(all.contains("fn sort"), "sort listed, got:\n{all}");
}

#[test]
fn implementors_and_cross_package_attachment() {
    let mut db = make_db();
    db.add_file("fixture.baml", FIXTURE);

    // Builtin interface → builtin implementors.
    let namespaces = Package::named(&db, "baml").namespaces(&db);
    let root = namespaces
        .iter()
        .find(|ns| ns.path(&db).is_empty())
        .unwrap();
    let Some((_, Symbol::Interface(comparable))) = root
        .items(&db)
        .into_iter()
        .find(|(name, _)| name.as_str() == "Comparable")
    else {
        panic!("baml.Comparable is an interface")
    };
    assert!(
        comparable.implementors(&db).len() >= 4,
        "int/bigint/string/float implement Comparable"
    );

    // User interface → the fixture's `implements Renderer for int` is found,
    // and the same impl attaches to baml.Int from the other direction.
    let user_root = *Package::user(&db).namespaces(&db).first().unwrap();
    let Some((_, Symbol::Interface(renderer))) = user_root
        .items(&db)
        .into_iter()
        .find(|(name, _)| name.as_str() == "Renderer")
    else {
        panic!("Renderer is an interface")
    };
    let implementors = renderer.implementors(&db);
    assert_eq!(implementors.len(), 1);
    assert_eq!(
        implementors[0]
            .for_ty(&db)
            .map(baml_type::Ty::render_canonical),
        Some("int".to_string())
    );

    let Some((_, Symbol::Class(int))) = root
        .items(&db)
        .into_iter()
        .find(|(name, _)| name.as_str() == "Int")
    else {
        panic!("baml.Int is a class")
    };
    let attached: Vec<String> = int
        .trait_impls(&db)
        .into_iter()
        .filter_map(|imp| imp.interface(&db))
        .map(|iface| iface.qualified_name(&db).render_dotted(false))
        .collect();
    assert!(
        attached.iter().any(|name| name == "user.Renderer"),
        "user impl attaches to baml.Int: {attached:?}"
    );
}

#[test]
fn in_body_impls_group_by_interface() {
    let db = make_db();
    let namespaces = Package::named(&db, "baml").namespaces(&db);
    let iter_ns = namespaces
        .iter()
        .find(|ns| {
            let path = ns.path(&db);
            path.len() == 1 && path[0].as_str() == "iter"
        })
        .expect("baml.iter namespace");
    let Some((_, Symbol::Class(array_iter))) = iter_ns
        .items(&db)
        .into_iter()
        .find(|(name, _)| name.as_str() == "ArrayIterator")
    else {
        panic!("baml.iter.ArrayIterator is a class")
    };

    let interfaces: Vec<String> = array_iter
        .trait_impls(&db)
        .into_iter()
        .filter_map(|imp| imp.interface(&db))
        .map(|iface| iface.qualified_name(&db).render_dotted(false))
        .collect();
    assert!(
        interfaces.contains(&"baml.iter.Iterable".to_string())
            && interfaces.contains(&"baml.iter.Iterator".to_string()),
        "in-body impls group by interface: {interfaces:?}"
    );
}
