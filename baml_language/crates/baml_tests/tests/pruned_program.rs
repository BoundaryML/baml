//! Behavioral equivalence matters more than structural validity: missing native
//! dependencies can silently choose a structural fallback. These oracles run
//! the same user/native fixtures through full and selected linking, then execute
//! serialized images. Test registration is removed using the parser so it cannot
//! accidentally root all declarations and defeat the pruning under test.
use baml_db::{LinkRoots, ProjectDatabase, SourceRootKind, baml_compiler_syntax::SyntaxKind};
use baml_test_support::{OptLevel, prefix};
use bex_engine::{BexExternalValue, UserFunctionCatalog};
use bex_vm_types::Program;

fn db_root(source: &str) -> (ProjectDatabase, baml_db::SourceRoot) {
    let db = baml_test_support::setup_test_db(source);
    let root = db
        .source_roots()
        .into_iter()
        .find(|r| r.kind(&db) == SourceRootKind::Workspace)
        .unwrap();
    (db, root)
}

fn round_trip(program: Program) -> Program {
    program.validate().unwrap();
    let decoded: Program = borsh::from_slice(&borsh::to_vec(&program).unwrap()).unwrap();
    decoded.validate().unwrap();
    decoded
}

#[tokio::test]
async fn native_file_interfaces_and_system_random() {
    let directory = tempfile::tempdir().unwrap();
    let path = serde_json::to_string(&directory.path().join("data.txt")).unwrap();
    let source = format!(
        r#"function Main() -> string {{
            let file = baml.fs.open({path}, "w+");
            defer {{ file.close(); }}
            let written = file.as<baml.io.Write>.write("hello");
            file.as<baml.io.Write>.flush();
            file.seek_from("start", 0);
            let text = file.as<baml.io.Read>.text();
            let rng = baml.random.SystemRandom.get();
            let bytes = rng.random(16);
            let number = rng.random_int();
            if (written != 5 || bytes.length() != 16) {{ throw "invalid I/O result"; }}
            text
        }}"#
    );
    let (full, small) = pair(&source, LinkRoots::EntryPoints(vec!["user.Main".into()]));
    assert!(small.objects.len() < full.objects.len());
    for program in [full, small] {
        assert_eq!(
            value(program, "user.Main").await,
            BexExternalValue::String("hello".into())
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn native_subprocess_pipe_interfaces() {
    let source = r#"function Main() -> string {
        let child = baml.sys.subprocess("/bin/cat", stdin = "pipe", stdout = "pipe", stderr = "ignore");
        defer { child.kill(); child.close(); }
        let input = child.stdin ?? throw "missing stdin";
        let output = child.stdout ?? throw "missing stdout";
        input.as<baml.io.Write>.write("hello");
        input.as<baml.io.Write>.flush();
        input.close();
        let text = output.as<baml.io.Read>.text();
        output.close();
        if (!child.wait().ok()) { throw "child failed"; }
        text
    }"#;
    let (full, small) = pair(source, LinkRoots::EntryPoints(vec!["user.Main".into()]));
    assert!(small.objects.len() < full.objects.len());
    for program in [full, small] {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(20),
            value(program, "user.Main"),
        )
        .await
        .unwrap();
        assert_eq!(result, BexExternalValue::String("hello".into()));
    }
}

#[tokio::test]
async fn native_tcp_interfaces() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let source = format!(
        r#"function Main() -> string {{
            let connection = baml.net.TcpStream.connect("{address}");
            defer {{ connection.close(); }}
            connection.as<baml.io.Write>.write("hello");
            connection.as<baml.io.Write>.flush();
            connection.as<baml.io.Read>.text()
        }}"#
    );
    let (full, small) = pair(&source, LinkRoots::EntryPoints(vec!["user.Main".into()]));
    assert!(small.objects.len() < full.objects.len());
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = [0; 5];
                stream.read_exact(&mut bytes).await.unwrap();
                assert_eq!(&bytes, b"hello");
                stream.write_all(&bytes).await.unwrap();
            }
        });
        for program in [full, small] {
            assert_eq!(
                value(program, "user.Main").await,
                BexExternalValue::String("hello".into())
            );
        }
        server.await.unwrap();
    })
    .await
    .unwrap();
}

fn pair(source: &str, roots: LinkRoots) -> (Program, Program) {
    pair_at(source, roots, OptLevel::Two)
}

fn pair_at(source: &str, roots: LinkRoots, opt: OptLevel) -> (Program, Program) {
    let (db, root) = db_root(source);
    baml_test_support::assert_no_user_diagnostic_errors(&db);
    let full = baml_db::compile_program_with(&db, root, opt, prefix(opt)).unwrap();
    let small =
        baml_db::compile_program_selected_with(&db, root, opt, prefix(opt), &roots).unwrap();
    (round_trip(full), round_trip(small))
}

async fn value(program: Program, entry: &str) -> BexExternalValue {
    baml_tests::engine::try_call_by_name(program, entry)
        .await
        .unwrap()
}

fn rewrite_tests(source: &str, callable: bool) -> (String, Vec<String>) {
    let (db, root) = db_root(source);
    let tree = baml_db::baml_compiler_parser::syntax_tree(&db, root.files(&db)[0]);
    let mut text = source.to_owned();
    let mut replacements = Vec::new();
    let mut roots = Vec::new();
    for test in tree
        .children()
        .filter(|n| n.kind() == SyntaxKind::TEST_EXPR_DEF)
    {
        let range = test.text_range();
        let replacement = if callable {
            let name = format!("PruningTest{}", roots.len());
            roots.push(format!("user.{name}"));
            let body = test
                .children()
                .find(|n| n.kind() == SyntaxKind::BLOCK_EXPR)
                .unwrap();
            format!("function {name}() -> void {}", body.text())
        } else {
            String::new()
        };
        replacements.push((
            usize::from(range.start())..usize::from(range.end()),
            replacement,
        ));
    }
    for (range, replacement) in replacements.into_iter().rev() {
        text.replace_range(range, &replacement);
    }
    (text, roots)
}

async fn native_corpus(source: &str) {
    let (source, roots) = rewrite_tests(source, true);
    assert!(!roots.is_empty());
    for opt in [OptLevel::One, OptLevel::Two] {
        let (full, small) = pair_at(&source, LinkRoots::EntryPoints(roots.clone()), opt);
        assert!(
            small.objects.len() < full.objects.len(),
            "fixture must exercise actual pruning"
        );
        for root in &roots {
            assert_eq!(
                value(full.clone(), root).await,
                BexExternalValue::Null,
                "baseline {root} at {opt:?}"
            );
            assert_eq!(
                value(small.clone(), root).await,
                BexExternalValue::Null,
                "pruned {root} at {opt:?}"
            );
        }
    }
}

macro_rules! native_fixture {
    ($name:ident, $path:literal) => {
        #[tokio::test]
        async fn $name() {
            native_corpus(include_str!($path)).await;
        }
    };
}
native_fixture!(
    native_toml_datetimes_and_errors,
    "../baml_src/ns_toml/toml.baml"
);
native_fixture!(
    native_csv_read_write_and_errors,
    "../baml_src/ns_csv/csv_smoke.baml"
);
native_fixture!(
    native_regex_construction_and_errors,
    "../baml_src/ns_regex/regex.baml"
);
native_fixture!(
    native_regex_replacement,
    "../baml_src/ns_regex/replace.baml"
);
native_fixture!(native_datetime, "../baml_src/ns_time/datetime.baml");
native_fixture!(native_crypto_sha256, "../baml_src/ns_crypto/sha2.baml");
native_fixture!(
    native_media_json,
    "../baml_src/ns_media_json_union/media_json_union.baml"
);

native_fixture!(
    native_structural_hashing_and_user_hashers,
    "../baml_src/ns_hash_runtime/hashing.baml"
);
native_fixture!(
    native_non_string_map_keys_and_callback_mutations,
    "../baml_src/ns_map_hash/map_hash.baml"
);
native_fixture!(
    native_map_hashing_equality_and_gc,
    "../baml_src/ns_hash_map_equality/hash_map_equality.baml"
);
#[tokio::test]
async fn implicit_structural_defaults() {
    let source = include_str!("../baml_src/ns_structural_defaults/structural_defaults.baml");
    // This fixture ends with compiler-diagnostic tests that depend on the
    // test-harness package. Only its preceding runtime assertions belong here.
    let end = source
        .find("test \"explicit_to_string_sources_remain_ambiguous\"")
        .expect("compiler-only fixture boundary");
    native_corpus(&source[..end]).await;
}

#[tokio::test]
async fn native_toml_edits_preserve_comments() {
    let source = r##"function Main() -> string {
        let doc = baml.toml.Table.parse("# project\n[toolchain] # selection\nversion = \"old\" # keep\n");
        let toolchain = doc.table("toolchain");
        toolchain.rename("version", "path");
        toolchain.set("path", "./cli");
        doc.to_string()
    }"##;
    for opt in [OptLevel::One, OptLevel::Two] {
        let (full, small) = pair_at(
            source,
            LinkRoots::EntryPoints(vec!["user.Main".into()]),
            opt,
        );
        assert!(small.objects.len() < full.objects.len());
        let expected = value(full, "user.Main").await;
        let BexExternalValue::String(text) = &expected else {
            panic!("expected TOML text: {expected:?}");
        };
        assert!(text.contains("# project"));
        assert!(text.contains("[toolchain] # selection"));
        assert!(text.contains("path = \"./cli\" # keep"));
        assert_eq!(value(small, "user.Main").await, expected);
    }
}

async fn semantic_corpus(source: &str) {
    let (source, _) = rewrite_tests(source, false);
    let (db, root) = db_root(&source);
    for opt in [OptLevel::One, OptLevel::Two] {
        let full = baml_db::compile_program_with(&db, root, opt, prefix(opt)).unwrap();
        let roots: Vec<_> = UserFunctionCatalog::from_program(&full)
            .unwrap()
            .user_functions()
            .into_iter()
            .filter(|f| f.param_names.is_empty() && f.display_type_params.is_empty())
            .map(|f| f.qualified_name)
            .collect();
        assert!(!roots.is_empty());
        let small = baml_db::compile_program_selected_with(
            &db,
            root,
            opt,
            prefix(opt),
            &LinkRoots::EntryPoints(roots.clone()),
        )
        .unwrap();
        let full = round_trip(full);
        let small = round_trip(small);
        assert!(small.objects.len() < full.objects.len());
        for root in roots {
            assert_eq!(
                value(full.clone(), &root).await,
                value(small.clone(), &root).await,
                "{root} at {opt:?}"
            );
        }
    }
}

#[tokio::test]
async fn nested_to_string_overrides() {
    semantic_corpus(include_str!(
        "../baml_src/ns_to_string_interface/to_string_interface.baml"
    ))
    .await;
}
#[tokio::test]
async fn from_json_overrides() {
    semantic_corpus(include_str!(
        "../baml_src/ns_from_json_interface/from_json_interface.baml"
    ))
    .await;
}
#[tokio::test]
async fn derived_json_and_cleanup_methods() {
    semantic_corpus(include_str!(
        "../baml_src/ns_json_auto_derive/json_auto_derive.baml"
    ))
    .await;
}

#[tokio::test]
async fn native_json_override_is_not_silently_replaced_by_structural_fallback() {
    let source = r#"class Special { n: int, implements baml.ToJson {
        function to_json(self) -> baml.json.json { baml.json.from(self.n + 5) }
    } }
    function Main() -> string { baml.json.stringify(baml.json.from(Special { n: 8 })) }
    function Dead() -> string { "never called" }
    "#;
    let (full, small) = pair(source, LinkRoots::EntryPoints(vec!["user.Main".into()]));
    assert!(small.objects.len() < full.objects.len());
    assert_eq!(
        value(full, "user.Main").await,
        BexExternalValue::String("13".into())
    );
    // Deliberately remove the rule: structural validation still passes, but
    // native JSON dispatch changes. This proves the execution oracle catches
    // the silent-fallback failure that structural validation cannot establish.
    let mut broken = small.clone();
    let to_json = broken
        .packages
        .iter()
        .find(|p| p.name.as_str() == "baml")
        .unwrap()
        .interfaces
        [&bex_vm_types::types::LocalName::new(Vec::new(), baml_base::Name::new("ToJson"))];
    broken.packages[broken.root as usize]
        .impl_rules
        .shift_remove(&to_json);
    broken.validate().unwrap();
    assert_eq!(
        value(broken, "user.Main").await,
        BexExternalValue::String("{\"n\":8}".into())
    );
    assert_eq!(
        value(small, "user.Main").await,
        BexExternalValue::String("13".into())
    );
}

#[tokio::test]
async fn reflection_falls_back_to_complete_image_and_keeps_working() {
    let source = "function Main() -> bool { reflect.Type.of<int>().implements(reflect.Type.of<baml.ops.Equals>()) }";
    let (full, selected) = pair(source, LinkRoots::EntryPoints(vec!["user.Main".into()]));
    assert_eq!(
        borsh::to_vec(&full).unwrap(),
        borsh::to_vec(&selected).unwrap()
    );
    assert_eq!(
        value(selected, "user.Main").await,
        BexExternalValue::Bool(true)
    );
}

#[tokio::test]
async fn sdk_host_surface_includes_independent_calls_and_runtime_compilation() {
    let source = r#"class Exposed { n: int }
    function Main() -> int { 4 }
    function HostOnly() -> int { 7 }
    function CompileAtRuntime() -> int {
        let package = reflect.Package.compile({ "main.baml": "function Answer() -> int { 42 }" });
        let session = reflect.Session.new(packages = { "compiled": package });
        session.eval<int>("compiled.Answer()")
    }"#;
    let (full, selected) = pair(source, LinkRoots::HostSurface);
    assert_eq!(
        borsh::to_vec(&full).unwrap(),
        borsh::to_vec(&selected).unwrap()
    );
    assert_eq!(
        value(selected.clone(), "user.HostOnly").await,
        BexExternalValue::Int(7)
    );
    assert_eq!(
        value(selected, "user.CompileAtRuntime").await,
        BexExternalValue::Int(42)
    );
}

#[tokio::test]
async fn generic_bounds_virtual_dispatch_type_switch_closures_and_gc_cleanup() {
    let source = r#"class Score { n: int,
        implements baml.ops.Equals { function eq(self, other: Self) -> bool throws never { self.n == other.n } }
        implements baml.ops.Compare { function cmp(self, other: Self) -> baml.ops.Ordering throws never { self.n.cmp(other.n) } }
    }
    function Sort<T extends baml.ops.Compare>(items: T[]) -> T[] { items.sort() }
    function Identity<T>(x: T) -> T { x }
    class Resource { log: string[], function cleanup(self) -> void throws never { self.log.push("cleaned") } }
    function Abandon(log: string[]) -> void throws never { let r = Resource { log }; r.log.push("created") }
    function Main() -> string {
        let get = Identity<int>;
        let sorted = Sort([Score { n: 3 }, Score { n: 1 }, Score { n: 2 }]);
        let values = sorted.map((x: Score) -> int { get(x.n) });
        let log: string[] = []; Abandon(log); baml.sys.collect_garbage();
        baml.json.stringify(baml.json.from([values.to_string(), log.to_string()]))
    }"#;
    let (full, small) = pair(source, LinkRoots::EntryPoints(vec!["user.Main".into()]));
    assert!(small.objects.len() < full.objects.len());
    assert_eq!(
        value(full, "user.Main").await,
        value(small, "user.Main").await
    );
}

async fn interface_corpus(source: &str, names: &[&str]) {
    let (source, _) = rewrite_tests(source, false);
    let roots: Vec<_> = names.iter().map(|n| format!("user.{n}")).collect();
    let (full, small) = pair(&source, LinkRoots::EntryPoints(roots.clone()));
    assert!(small.objects.len() < full.objects.len());
    for root in roots {
        assert_eq!(
            value(full.clone(), &root).await,
            value(small.clone(), &root).await,
            "{root}"
        );
    }
}

#[tokio::test]
async fn interfaces_blanket_rules_and_receiver_instantiations() {
    interface_corpus(
        include_str!("../baml_src/ns_interfaces/interfaces_3.baml"),
        &[
            "w301_main",
            "w302_main",
            "w304_main",
            "w311_main",
            "w312_main",
            "w3sp_main",
            "w3oo_main",
            "w3bm_main",
            "w3um_main",
            "w3cf_main",
            "ufp1_main",
            "ufp3_main",
            "ufp4_main",
            "uf01_main",
            "uf02_main",
            "uf05_main",
            "uf08_main",
            "uf09_main",
            "uf10_main",
            "ptu_main",
            "vdc_dispatch_distinguishes_instantiations",
        ],
    )
    .await;
}

#[tokio::test]
async fn associated_types_bounds_and_default_methods() {
    interface_corpus(
        include_str!("../baml_src/ns_interfaces_associated_types/interfaces_associated_types.baml"),
        &[
            "atp_throws_main",
            "cae_main",
            "dms_main",
            "bsp_main",
            "bcp_main",
            "upc_main",
            "adb_main",
            "apd_main",
            "ing_main",
            "rds_main",
            "rdn_main",
            "rmf_main",
            "rdf_main",
            "srab_main",
        ],
    )
    .await;
}
