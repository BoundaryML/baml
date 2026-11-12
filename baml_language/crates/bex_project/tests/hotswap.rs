//! `BexEngine::hotswap_patch`: installing a new version of a running program
//! (BEP-087, stage 1).
//!
//! Each test starts an engine on `SOURCE`, edits the source, patches, and calls
//! functions from the host, the way a host or `baml run --hot` would.

use std::{
    io::{BufRead as _, BufReader, Read as _, Write as _},
    sync::Arc,
};

use bex_engine::{
    BexEngine, BexExternalValue, CallId, EngineConfig, FunctionCallContext,
    FunctionCallContextBuilder, HotswapError, WireTy,
};
use indexmap::IndexMap;
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
class Counter {
    n: int,

    function add(self, x: int) -> int { self.n + x }
}

interface Describe {
    function describe(self) -> string throws never;
}

implement Describe for Counter {
    function describe(self) -> string throws never { "old describe" }
}

function describe_any(d: Describe) -> string { d.describe() }

enum Color {
    Red
    Green
}

class Log {
    inits: int,
}

const InitLog = Log { inits: 0 };

function count_init(log: Log) -> int {
    log.inits += 1;
    log.inits
}

const Tracked = count_init(InitLog);

function inits() -> int { InitLog.inits }

function unused() -> int { 1 }

function helper(x: int) -> int { x + 1 }

function uses_helper(x: int) -> int { helper(x) * 10 }

function label<T>(x: T) -> string { "old " + reflect.Type.of<T>().to_string() }

function make_counter() -> int {
    let c = Counter { n: 7 };
    if (c is Counter) { c.add(1) } else { 0 }
}

function method_call() -> int { Counter { n: 1 }.add(1) }

function virtual_call() -> string { describe_any(Counter { n: 1 }) }

function hook_decide(x: int, settings: trace.Settings) -> trace.Options? throws never {
    InitLog.inits += 100;
    null
}

/// baml:$trace=hook_decide
function hooked(x: int) -> int { x }

function wait_for(path: string) -> null {
    while (!baml.fs.exists(path)) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(5));
    }
    null
}

/// Holds a function value from before a patch, and calls through it after.
function in_flight(gate: string, entered: string) -> string {
    let f = helper;
    baml.fs.write(entered, "") catch_all (e) { _ => 0 };
    wait_for(gate);
    "old body, value reaches " + string.from(f(1))
}

function spawned() -> int {
    let task = spawn { helper(1) };
    await task
}

function collect() -> int {
    baml.sys.collect_garbage();
    helper(1)
}
"#;

/// An engine running `source` as `main.baml`, with its source recorded.
fn engine(source: &str) -> Arc<BexEngine> {
    engine_files(&[("main.baml", source)])
}

/// An engine running `files`, with their source recorded.
fn engine_files(files: &[(&str, &str)]) -> Arc<BexEngine> {
    // As `baml run --hot` does before it compiles.
    baml_env::set_hotswap_enabled();
    let program = baml_test_support::compile_multi_file(files);
    let baseline =
        bex_project::hotswap_fingerprint::declaration_fingerprints(files.iter().copied());
    Arc::new(
        BexEngine::new_with_config(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            EngineConfig {
                runtime_compiler: Some(bex_project::runtime_compiler()),
                hotswap_baseline: Some(baseline),
                ..EngineConfig::default()
            },
        )
        .expect("engine"),
    )
}

fn context(type_args: &[(&str, WireTy)]) -> FunctionCallContext {
    FunctionCallContextBuilder::new(CallId::next())
        .with_type_args(
            type_args
                .iter()
                .map(|(name, ty)| ((*name).to_string(), ty.clone()))
                .collect(),
        )
        .build()
}

async fn call(
    engine: &Arc<BexEngine>,
    name: &str,
    args: Vec<BexExternalValue>,
) -> BexExternalValue {
    engine
        .call_function(name, args, context(&[]), true)
        .await
        .unwrap_or_else(|error| panic!("{name}: {error}"))
}

async fn int(engine: &Arc<BexEngine>, name: &str, arg: Option<i64>) -> i64 {
    let args = arg.map(BexExternalValue::Int).into_iter().collect();
    match call(engine, name, args).await {
        BexExternalValue::Int(value) => value,
        other => panic!("{name} returned {other:?}"),
    }
}

async fn text(engine: &Arc<BexEngine>, name: &str) -> String {
    match call(engine, name, Vec::new()).await {
        BexExternalValue::String(value) => value.to_string(),
        other => panic!("{name} returned {other:?}"),
    }
}

async fn patch(engine: &Arc<BexEngine>, source: &str) -> Result<Vec<String>, HotswapError> {
    patch_files(engine, &[("main.baml", source)]).await
}

async fn patch_files(
    engine: &Arc<BexEngine>,
    files: &[(&str, &str)],
) -> Result<Vec<String>, HotswapError> {
    let files: IndexMap<String, String> = files
        .iter()
        .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
        .collect();
    engine.hotswap_patch(files).await
}

fn rejected(result: Result<Vec<String>, HotswapError>, needle: &str) {
    match result {
        Err(error @ HotswapError::Rejected(_)) => {
            assert!(error.to_string().contains(needle), "{error}");
        }
        other => panic!("expected a rejection naming {needle:?}, got {other:?}"),
    }
}

/// `source` with `from` replaced by `to`, which must occur in it.
fn edit(source: &str, from: &str, to: &str) -> String {
    assert!(source.contains(from), "{from:?} is not in the source");
    source.replacen(from, to, 1)
}

#[tokio::test(flavor = "multi_thread")]
async fn patch_swaps_changed_bodies_and_leaves_the_rest_alone() {
    let engine = engine(SOURCE);

    // Comments and formatting change nothing.
    let source = edit(
        SOURCE,
        "function helper(x: int) -> int { x + 1 }",
        "// note\nfunction helper(x: int) -> int {\n    x + 1\n}",
    );
    assert_eq!(patch(&engine, &source).await.unwrap(), Vec::<String>::new());

    // A body swaps, and a caller that did not change reaches it.
    let source = edit(&source, "    x + 1\n", "    x + 5\n");
    assert_eq!(patch(&engine, &source).await.unwrap(), ["helper"]);
    assert_eq!(int(&engine, "helper", Some(1)).await, 6);
    assert_eq!(int(&engine, "uses_helper", Some(1)).await, 60);

    // A patch reports what changed since the last one, and the same source
    // again changes nothing: the swap stays installed.
    assert_eq!(patch(&engine, &source).await.unwrap(), Vec::<String>::new());
    assert_eq!(int(&engine, "helper", Some(1)).await, 6);

    // A method, an implementation reached by virtual dispatch, and a generic.
    let source = edit(&source, "{ self.n + x }", "{ self.n * 50 + x }");
    let source = edit(&source, "{ \"old describe\" }", "{ \"new describe\" }");
    let source = edit(&source, "{ \"old \" + reflect", "{ \"new \" + reflect");
    let mut names = patch(&engine, &source).await.unwrap();
    names.sort();
    assert_eq!(
        names,
        ["Counter.add", "Describe for Counter.describe", "label"]
    );
    assert_eq!(int(&engine, "method_call", None).await, 51);
    assert_eq!(text(&engine, "virtual_call").await, "new describe");
    for (arg, ty, expected) in [
        (BexExternalValue::Int(1), WireTy::int(), "new int"),
        (
            BexExternalValue::String("a".into()),
            WireTy::string(),
            "new string",
        ),
    ] {
        // A host call enters the patched generic with its own type arguments.
        let value = engine
            .call_function("label", vec![arg], context(&[("T", ty)]), true)
            .await
            .expect("label");
        assert!(matches!(value, BexExternalValue::String(s) if s.to_string() == expected));
    }

    // A patched body builds the running class: its identity did not fork.
    let source = edit(&source, "Counter { n: 7 }", "Counter { n: 8 }");
    assert_eq!(patch(&engine, &source).await.unwrap(), ["make_counter"]);
    assert_eq!(int(&engine, "make_counter", None).await, 401);

    // A function the source leaves as built returns to its own body.
    let source = edit(&source, "{ self.n * 50 + x }", "{ self.n + x }");
    assert_eq!(patch(&engine, &source).await.unwrap(), ["Counter.add"]);
    assert_eq!(int(&engine, "method_call", None).await, 2);
    assert_eq!(int(&engine, "make_counter", None).await, 9);
    let mut names = patch(&engine, SOURCE).await.unwrap();
    names.sort();
    assert_eq!(
        names,
        [
            "Describe for Counter.describe",
            "helper",
            "label",
            "make_counter"
        ]
    );
    assert_eq!(int(&engine, "helper", Some(1)).await, 2);
    assert_eq!(text(&engine, "virtual_call").await, "old describe");
    assert_eq!(int(&engine, "make_counter", None).await, 8);
    assert_eq!(patch(&engine, SOURCE).await.unwrap(), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn edits_that_need_a_restart_install_nothing() {
    let engine = engine(SOURCE);
    // A good body edit rides along with each rejected one, and never lands.
    let pending = edit(SOURCE, "{ x + 1 }", "{ x + 9 }");
    for (from, to, needle) in [
        (
            "    inits: int,\n",
            "    inits: int,\n    extra: int?,\n",
            "`Log` changed, which changes the public interface",
        ),
        ("    Green\n", "    Green\n    Blue\n", "`Color` changed"),
        (
            "Log { inits: 0 }",
            "Log { inits: 5 }",
            "the value of `InitLog` changed",
        ),
        ("function unused() -> int { 1 }", "", "`unused` was deleted"),
        (
            "function helper(x: int)",
            "function helper(x: int, y: int = 0)",
            "`helper` cannot be swapped, which needs a restart",
        ),
        (
            "function unused",
            "function brand_new() -> int { 1 }\nfunction unused",
            "`brand_new` is new",
        ),
        (
            "class Log {",
            "class Fresh {\n    n: int,\n}\nclass Log {",
            "`Fresh` is new",
        ),
        (
            "    function add(self, x: int)",
            "    function other(self) -> int { 1 }\n    function add(self, x: int)",
            "`Counter.other` is new",
        ),
    ] {
        let error = patch(&engine, &edit(&pending, from, to)).await.unwrap_err();
        assert!(
            matches!(&error, HotswapError::Rejected(_)) && error.to_string().contains(needle),
            "{from:?}: {error}"
        );
        assert_eq!(int(&engine, "helper", Some(1)).await, 2);
    }
    let error = patch(&engine, &edit(&pending, "{ x + 9 }", "{ x + }"))
        .await
        .unwrap_err();
    assert!(matches!(error, HotswapError::Compile(_)), "{error}");
    assert_eq!(int(&engine, "helper", Some(1)).await, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_call_in_flight_finishes_on_the_body_it_entered() {
    let engine = engine(SOURCE);
    let dir = tempfile::tempdir().expect("tempdir");
    let gate = dir.path().join("gate").to_string_lossy().into_owned();
    let entered = dir.path().join("entered").to_string_lossy().into_owned();
    let running = {
        let engine = Arc::clone(&engine);
        let args = vec![
            BexExternalValue::String(gate.clone().into()),
            BexExternalValue::String(entered.clone().into()),
        ];
        tokio::spawn(async move {
            engine
                .call_function("in_flight", args, context(&[]), true)
                .await
        })
    };
    while !std::path::Path::new(&entered).exists() {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let source = edit(SOURCE, "{ x + 1 }", "{ x + 100 }");
    let source = edit(
        &source,
        "\"old body, value reaches \"",
        "\"new body, value reaches \"",
    );
    let mut names = patch(&engine, &source).await.unwrap();
    names.sort();
    assert_eq!(names, ["helper", "in_flight"]);
    std::fs::write(&gate, "").unwrap();
    // The suspended call keeps its body, and a function value it took before
    // the patch reaches the replacement.
    let value = running.await.unwrap().expect("in_flight");
    assert!(
        matches!(value, BexExternalValue::String(s) if s.to_string() == "old body, value reaches 101")
    );
    assert_eq!(int(&engine, "spawned", None).await, 101);
    // The replacement's package is reachable only through the redirect.
    assert_eq!(int(&engine, "collect", None).await, 101);
}

/// How much one call of `name` moves `hook_decide`'s counter.
async fn hook_runs(engine: &Arc<BexEngine>, name: &str) -> i64 {
    let before = int(engine, "inits", None).await;
    int(engine, name, Some(1)).await;
    int(engine, "inits", None).await - before
}

/// A swap replaces a function's declared hook along with its body: the call
/// enters the replacement, and the replacement's own hook runs.
#[tokio::test(flavor = "multi_thread")]
async fn a_swap_replaces_the_declared_trace_hook() {
    let engine = engine(SOURCE);
    let per_call = hook_runs(&engine, "hooked").await;
    assert!(per_call > 0);
    assert_eq!(hook_runs(&engine, "helper").await, 0);

    // A body edit keeps the hook it declares.
    let source = edit(SOURCE, "{ x }", "{ x + 1 }");
    assert_eq!(patch(&engine, &source).await.unwrap(), ["hooked"]);
    assert_eq!(int(&engine, "hooked", Some(1)).await, 2);
    assert_eq!(hook_runs(&engine, "hooked").await, per_call);

    // Removing the hook, and adding one to another function.
    let source = edit(&source, "/// baml:$trace=hook_decide\n", "");
    let source = edit(
        &source,
        "function helper(x: int)",
        "/// baml:$trace=hook_decide\nfunction helper(x: int)",
    );
    let mut names = patch(&engine, &source).await.unwrap();
    names.sort();
    assert_eq!(names, ["helper", "hooked"]);
    assert_eq!(hook_runs(&engine, "hooked").await, 0);
    assert_eq!(hook_runs(&engine, "helper").await, per_call);
    assert_eq!(int(&engine, "hooked", Some(1)).await, 2);
}

/// A local server speaking enough of the Anthropic API to answer `count`
/// requests, reporting each request's body.
fn stub_llm(count: usize) -> (String, std::sync::mpsc::Receiver<String>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (sender, bodies) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(count) {
            let mut stream = stream.expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("header");
                if line.trim().is_empty() {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().expect("length");
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).expect("body");
            let _ = sender.send(String::from_utf8_lossy(&body).into_owned());
            let reply = r#"{"id":"m","type":"message","role":"assistant","model":"m","content":[{"type":"text","text":"reply"}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                reply.len()
            )
            .expect("write");
        }
    });
    (url, bodies)
}

/// The whole LLM path through a patch, with a return type whose companions
/// must match the running program's enum.
#[tokio::test(flavor = "multi_thread")]
async fn a_patched_prompt_goes_out_on_the_wire() {
    let (url, bodies) = stub_llm(2);
    let source = format!(
        r#"client Local = anthropic.Client.new(model = "m", api_key = "k", base_url = "{url}");

enum Queue {{
    reply
    other
}}

function Route(ticket: string) -> Queue {{
    client: Local
    prompt: `old route ${{ticket}}
${{ctx.output_format()}}`
}}

function routed() -> bool {{
    Route("t") == Queue.reply
}}
"#
    );
    let engine = engine(&source);
    assert!(matches!(
        call(&engine, "routed", Vec::new()).await,
        BexExternalValue::Bool(true)
    ));
    let patched = edit(&source, "`old route", "`new route");
    assert_eq!(patch(&engine, &patched).await.unwrap(), ["Route"]);
    assert!(matches!(
        call(&engine, "routed", Vec::new()).await,
        BexExternalValue::Bool(true)
    ));
    assert!(bodies.recv().unwrap().contains("old route t"));
    assert!(bodies.recv().unwrap().contains("new route t"));
}

#[tokio::test(flavor = "multi_thread")]
async fn implementations_with_the_same_bare_names_are_each_patched() {
    let main = r#"
interface Show {
    function show(self) -> string throws never;
}
class Foo {
    n: int,
}
implement Show for Foo {
    function show(self) -> string throws never { "root old" }
}
function show_root() -> string {
    let s: Show = Foo { n: 1 };
    s.show()
}
"#;
    let nested = r#"
class Foo {
    n: int,
}
implement root.Show for Foo {
    function show(self) -> string throws never { "nested" }
}
"#;
    let engine = engine_files(&[("main.baml", main), ("ns_a/a.baml", nested)]);
    assert_eq!(text(&engine, "show_root").await, "root old");
    let main = edit(main, "\"root old\"", "\"root new\"");
    let names = patch_files(&engine, &[("main.baml", &main), ("ns_a/a.baml", nested)])
        .await
        .unwrap();
    assert_eq!(names, ["Show for Foo.show"]);
    assert_eq!(text(&engine, "show_root").await, "root new");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_change_reaches_clients_built_from_it() {
    let (url, bodies) = stub_llm(2);
    let source = format!(
        r#"client Base = anthropic.Client.new(model = "model-one", api_key = "k", base_url = "{url}");
client Wrapped = ai.clients.Retry.new(Base, max_attempts = 1);

function AskWrapped(q: string) -> string {{
    client: Wrapped
    prompt: `wrapped ${{q}}`
}}

function ask_wrapped() -> string {{
    AskWrapped("x")
}}
"#
    );
    let engine = engine(&source);
    assert_eq!(text(&engine, "ask_wrapped").await, "reply");
    assert!(bodies.recv().unwrap().contains("model-one"));
    let patched = edit(&source, "model-one", "model-two");
    assert_eq!(patch(&engine, &patched).await.unwrap(), ["AskWrapped"]);
    assert_eq!(text(&engine, "ask_wrapped").await, "reply");
    assert!(bodies.recv().unwrap().contains("model-two"));
}

/// A replacement declares exactly the signature as built: a wider or a
/// narrower one would change the public interface.
#[tokio::test(flavor = "multi_thread")]
async fn a_replacement_must_keep_the_exact_signature() {
    let source = "function show(x: int | string) -> string { \"x\" }\n";
    let engine = engine(source);
    for (from, to) in [
        ("x: int | string", "x: int"),
        ("x: int | string", "x: int | string | bool"),
        ("-> string {", "-> string | null {"),
    ] {
        rejected(
            patch(&engine, &edit(source, from, to)).await,
            "`show` cannot be swapped",
        );
    }
    let body = edit(source, "{ \"x\" }", "{ \"y\" }");
    assert_eq!(patch(&engine, &body).await.unwrap(), ["show"]);

    // The runtime widens a thrown literal to `string`, so the declared clause
    // is what changes.
    let source = "function f(x: int) -> int throws \"old\" { if (x > 0) { throw \"old\" } x }\n";
    let throwing = self::engine(source);
    rejected(
        patch(&throwing, &source.replace("\"old\"", "\"new\"")).await,
        "`f` changed its declared `throws`",
    );
}

/// The compiler takes namespaces from directories only.
#[tokio::test(flavor = "multi_thread")]
async fn a_file_named_like_a_namespace_is_not_one() {
    let engine = engine_files(&[("ns_main.baml", SOURCE)]);
    let source = edit(SOURCE, "{ x + 1 }", "{ x + 5 }");
    let names = patch_files(&engine, &[("ns_main.baml", &source)])
        .await
        .unwrap();
    assert_eq!(names, ["helper"]);
    assert_eq!(int(&engine, "helper", Some(1)).await, 6);
}

/// A function value from before a swap equals the same function named in a
/// swapped body: the swap's package names it through its own slot.
#[tokio::test(flavor = "multi_thread")]
async fn a_function_value_stays_equal_to_its_name_across_a_swap() {
    let source = "function helper(x: int) -> int { x }\nconst Fns = [helper];\nfunction check() -> bool { Fns[0] == helper }\n";
    let engine = engine(source);
    assert!(matches!(
        call(&engine, "check", Vec::new()).await,
        BexExternalValue::Bool(true)
    ));
    let patched = edit(source, "Fns[0] == helper }", "Fns[0] == helper && true }");
    assert_eq!(patch(&engine, &patched).await.unwrap(), ["check"]);
    assert!(matches!(
        call(&engine, "check", Vec::new()).await,
        BexExternalValue::Bool(true)
    ));
}

/// An edit the running program cannot place is rejected, never lost: source
/// names an implementation by the alias it wrote, the program by the type.
#[tokio::test(flavor = "multi_thread")]
async fn an_edit_with_no_running_function_is_rejected() {
    let source = "interface Show { function show(self) -> string throws never; }\ntype Ints = int[];\nimplement Show for Ints { function show(self) -> string throws never { \"old\" } }\nfunction run() -> string { let s: Show = [1]; s.show() }\n";
    let engine = engine(source);
    rejected(
        patch(&engine, &edit(source, "\"old\"", "\"new\"")).await,
        "`Show for Ints.show` changed, but no running function could be matched to it",
    );
    assert_eq!(text(&engine, "run").await, "old");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_instance_method_cannot_become_a_static_one() {
    let source = "class Foo {\n    n: int,\n\n    function m(self) -> int { self.n }\n}\n";
    let engine = engine(source);
    rejected(
        patch(
            &engine,
            &edit(
                source,
                "m(self) -> int { self.n }",
                "m(x: Foo) -> int { x.n }",
            ),
        )
        .await,
        "not named like",
    );
}

/// A client built through a helper is rebuilt too, and a client's name inside
/// a string is not a use of it.
#[tokio::test(flavor = "multi_thread")]
async fn a_client_change_follows_helpers_and_not_literals() {
    let (url, bodies) = stub_llm(2);
    let source = format!(
        r#"client Base = anthropic.Client.new(model = "model-one", api_key = "k", base_url = "{url}");
const Message = "use Base here";

function get_base() -> ai.Client {{ Base }}

client Wrapped = ai.clients.Retry.new(get_base(), max_attempts = 1);

function Ask(q: string) -> string {{
    client: Wrapped
    prompt: `ask ${{q}}`
}}

function ask() -> string {{
    Ask("x")
}}
"#
    );
    let engine = engine(&source);
    assert_eq!(text(&engine, "ask").await, "reply");
    assert!(bodies.recv().unwrap().contains("model-one"));
    let mut names = patch(&engine, &edit(&source, "model-one", "model-two"))
        .await
        .unwrap();
    names.sort();
    assert_eq!(names, ["Ask", "get_base"]);
    assert_eq!(text(&engine, "ask").await, "reply");
    assert!(bodies.recv().unwrap().contains("model-two"));
}

/// A changed client builds once per change: a later save that leaves it alone
/// reuses it, even while it changes another, and a client back as built is
/// the running one.
#[tokio::test(flavor = "multi_thread")]
async fn a_changed_client_builds_once_per_change() {
    let source = r#"class Log {
    inits: int,
}

const InitLog = Log { inits: 0 };

function make_base(model: string) -> ai.Client {
    InitLog.inits += 1;
    anthropic.Client.new(model = model, api_key = "k")
}

client Base = make_base("model-one");
client Other = make_base("other-one");

function Ask(q: string) -> string {
    client: Base
    prompt: `ask ${q}`
}

function AskOther(q: string) -> string {
    client: Other
    prompt: `ask ${q}`
}

function inits() -> int { InitLog.inits }
"#;
    let engine = engine(source);
    assert_eq!(int(&engine, "inits", None).await, 2);
    let changed = edit(source, "model-one", "model-two");
    assert_eq!(patch(&engine, &changed).await.unwrap(), ["Ask"]);
    assert_eq!(int(&engine, "inits", None).await, 3);
    // A comment changes no client, so it reuses the one just built.
    let commented = edit(&changed, "function inits", "// note\nfunction inits");
    assert_eq!(
        patch(&engine, &commented).await.unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(int(&engine, "inits", None).await, 3);
    // Changing another client builds only that one.
    let other = edit(&commented, "other-one", "other-two");
    assert_eq!(patch(&engine, &other).await.unwrap(), ["AskOther"]);
    assert_eq!(int(&engine, "inits", None).await, 4);
    // A different value is built again.
    let third = edit(&other, "model-two", "model-three");
    assert_eq!(patch(&engine, &third).await.unwrap(), ["Ask"]);
    assert_eq!(int(&engine, "inits", None).await, 5);
    // As built, the running clients serve, and nothing is built.
    let mut names = patch(&engine, source).await.unwrap();
    names.sort();
    assert_eq!(names, ["Ask", "AskOther"]);
    assert_eq!(int(&engine, "inits", None).await, 5);
}
