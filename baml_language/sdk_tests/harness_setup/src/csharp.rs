//! C# fixture build setup.

use std::{env, fs, path::PathBuf};

use baml_db::{ProjectDatabase, SourceRootSpec, baml_compiler_diagnostics::Severity};

use crate::{emit_cargo_line, watch_dir};

pub fn run_all() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    generate_fixture(
        &manifest_dir,
        "basic_calls",
        "sdk_test_csharp.basic_calls",
        "BasicCalls.csproj",
    );
    generate_fixture(
        &manifest_dir,
        "type_roundtrips",
        "sdk_test_csharp.type_roundtrips",
        "TypeRoundtrips.csproj",
    );
    generate_fixture(
        &manifest_dir,
        "generics",
        "sdk_test_csharp.generics",
        "Generics.csproj",
    );
    generate_fixture(
        &manifest_dir,
        "failures_and_cancellation",
        "sdk_test_csharp.failures_and_cancellation",
        "FailuresAndCancellation.csproj",
    );
    generate_fixture(
        &manifest_dir,
        "media",
        "sdk_test_csharp.media",
        "Media.csproj",
    );
    generate_fixture(
        &manifest_dir,
        "streaming",
        "sdk_test_csharp.streaming",
        "Streaming.csproj",
    );
    generate_fixture(
        &manifest_dir,
        "host_callables",
        "sdk_test_csharp.host_callables",
        "HostCallables.csproj",
    );
    generate_fixture(
        &manifest_dir,
        "stdlib_resources",
        "sdk_test_csharp.stdlib_resources",
        "StdlibResources.csproj",
    );
    generate_fixture(
        &manifest_dir,
        "dynamic_values",
        "sdk_test_csharp.dynamic_values",
        "DynamicValues.csproj",
    );
    generate_fixture(
        &manifest_dir,
        "primitive_edges",
        "sdk_test_csharp.primitive_edges",
        "PrimitiveEdges.csproj",
    );
    generate_fixture(
        &manifest_dir,
        "stdlib_structurals",
        "sdk_test_csharp.stdlib_structurals",
        "StdlibStructurals.csproj",
    );
    emit_cargo_line(format_args!("cargo:rerun-if-changed=build.rs"));
}

fn generate_fixture(
    manifest_dir: &std::path::Path,
    fixture_name: &str,
    program_identity: &str,
    project_file: &str,
) {
    let fixture = manifest_dir.join(fixture_name);
    let baml_src = fixture.join("baml_src");
    let canonical = fs::canonicalize(&baml_src)
        .unwrap_or_else(|error| panic!("failed to locate {}: {error}", baml_src.display()));

    let mut db = ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let root = db
        .add_source_root(SourceRootSpec {
            path: canonical.clone(),
            package: baml_db::Name::new(baml_type::RESERVED_USER_PACKAGE),
            kind: baml_db::SourceRootKind::Workspace,
        })
        .unwrap_or_else(|error| panic!("C# {fixture_name}: cannot add workspace root: {error}"));
    let baml_files = baml_db::discover_baml_files(&canonical);
    assert!(
        !baml_files.is_empty(),
        "C# {fixture_name} fixture has no BAML files"
    );
    for path in &baml_files {
        let source = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        db.add_or_update_file_in(root, path, &source);
    }

    let diagnostics = baml_db::collect_diagnostics(&db);
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "C# {fixture_name} fixture diagnostics: {errors:#?}"
    );

    let symbols = baml_ide::build_symbol_pool(&db);
    let bytecode = borsh::to_vec(
        &db.get_bytecode()
            .unwrap_or_else(|error| panic!("C# bytecode compilation failed: {error:?}")),
    )
    .expect("C# fixture bytecode serialization failed");
    let output_directory = fixture.join("baml_sdk");
    let embedded_baml_toml = format!(
        "[__baml_codegen]\nmetadata_version = 1\n\n[__baml_codegen.toolchain]\nversion = {:?}\n",
        baml_version::CANONICAL_VERSION
    );
    let generate = || {
        sdkgen_csharp::generate_into(sdkgen_csharp::CSharpGenerateRequest {
            symbols: &symbols,
            program_bytes: &bytecode,
            embedded_baml_toml: &embedded_baml_toml,
            cli_version: baml_version::CANONICAL_VERSION,
            required_bridge_version: baml_version::CANONICAL_VERSION,
            program_identity,
            output_directory: output_directory.clone(),
        })
    };
    let first =
        generate().unwrap_or_else(|error| panic!("C# {fixture_name} generation failed: {error}"));
    let second = generate()
        .unwrap_or_else(|error| panic!("C# {fixture_name} deterministic repeat failed: {error}"));
    assert_eq!(
        first.manifest, second.manifest,
        "C# repeat generation manifest changed"
    );
    if fixture_name == "generics" {
        verify_generics_surface(&fixture);
    }
    if fixture_name == "host_callables" {
        verify_host_callables_surface(&fixture);
    }
    if fixture_name == "stdlib_resources" {
        verify_stdlib_resources_surface(&fixture);
    }

    watch_dir(&baml_src);
    for path in ["baml.toml", "Program.cs", project_file] {
        emit_cargo_line(format_args!(
            "cargo:rerun-if-changed={}",
            fixture.join(path).display()
        ));
    }
}

fn verify_generics_surface(fixture: &std::path::Path) {
    let generated = fixture.join("baml_sdk").join("CsharpGenerics");
    let functions = fs::read_to_string(generated.join("Functions.g.cs"))
        .expect("failed to read generated generics function surface");
    let box_source = fs::read_to_string(generated.join("Box.g.cs"))
        .expect("failed to read generated generics generic class surface");
    let counter = fs::read_to_string(generated.join("Counter.g.cs"))
        .expect("failed to read generated generics nongeneric method surface");
    let pair = fs::read_to_string(generated.join("Pair.g.cs"))
        .expect("failed to read generated generics multi-generic surface");

    for expected in [
        "public static T Identity<T>(",
        "public static string TypeName<T>(",
        "public static global::Baml.BamlNullable<T> Maybe<T>(",
        "public static T LocalCollision<T>(",
    ] {
        assert!(
            functions.contains(expected),
            "generated generics function surface omitted `{expected}`"
        );
    }
    for forbidden in ["_types", "Type[]", "object? value"] {
        assert!(
            !functions.contains(forbidden),
            "generated generics public function surface exposed `{forbidden}`"
        );
    }
    for expected in [
        "public T Get(",
        "Replace<U>(",
        "public static global::CsharpGenerics.Box<V> New<V>(",
        "this);",
    ] {
        assert!(
            box_source.contains(expected),
            "generated generics generic method surface omitted `{expected}`"
        );
    }
    assert!(counter.contains("public long Add("));
    assert!(counter.contains("public static global::CsharpGenerics.Counter New("));
    assert!(pair.contains("public B GetSecond("));
    assert!(pair.contains("ReplaceSecond<C>("));
}

fn verify_host_callables_surface(fixture: &std::path::Path) {
    let generated = fixture.join("baml_sdk").join("CsharpHostCallables");
    let functions = fs::read_to_string(generated.join("Functions.g.cs"))
        .expect("failed to read generated host-callable function surface");
    let callback_box = fs::read_to_string(generated.join("CallbackBox.g.cs"))
        .expect("failed to read generated host-callable generic callback method surface");
    let callback_host = fs::read_to_string(generated.join("CallbackHost.g.cs"))
        .expect("failed to read generated host-callable static generic callback surface");

    for expected in [
        "Task<R> ApplyAsync<T, R>(",
        "global::System.Func<T, global::System.Threading.CancellationToken, global::System.Threading.Tasks.Task<R>> callback",
        ".AddHostCallable(",
        ".Required(bamlType0)",
        ".Result(bamlType1)",
        ".Optional(\"fallback\", bamlType0)",
        ".VoidResult()",
        "global::System.Func<global::System.Threading.CancellationToken, global::System.Threading.Tasks.Task> callback",
    ] {
        assert!(
            functions.contains(expected),
            "generated host-callable generic callback surface omitted `{expected}`"
        );
    }
    assert!(
        !functions.contains("ResolveType<global::System.Func<"),
        "generated host-callable callback attempted to infer wire metadata from CLR Func"
    );
    assert!(callback_box.contains("TransformAsync<R>("));
    assert!(callback_box.contains(".AddHostCallable("));
    assert!(
        callback_host
            .contains("public static global::System.Threading.Tasks.Task<R> ApplyAsync<T, R>(")
    );
    assert!(callback_host.contains(".AddHostCallable("));
    assert!(
        !functions.contains("global::System.Threading.Tasks.ValueTask")
            && !functions.contains("global::System.Action<")
            && !functions.contains("global::Baml.BamlCallback"),
        "generated host-callable surface introduced an alternate callback family instead of retaining the canonical Task-based Func"
    );
}

fn verify_stdlib_resources_surface(fixture: &std::path::Path) {
    let generated = fixture.join("baml_sdk").join("Baml");
    let file = fs::read_to_string(generated.join("Fs").join("File.g.cs"))
        .expect("failed to read generated typed File resource surface");
    let fs_functions = fs::read_to_string(generated.join("Fs").join("Functions.g.cs"))
        .expect("failed to read generated baml.fs function surface");
    let glob = fs::read_to_string(generated.join("Glob").join("Glob.g.cs"))
        .expect("failed to read generated typed Glob resource surface");

    for expected in [
        "public sealed partial class File : global::System.IDisposable",
        "public string Read(",
        "public long SeekFrom(\n        string whence,",
        "public string Text(",
        "public File Clone() => new(\n        resource.Clone());",
    ] {
        assert!(
            file.contains(expected),
            "generated File resource surface omitted `{expected}`"
        );
    }
    for expected in [
        "namespace Baml.Fs;",
        "public static global::Baml.Fs.File Open(",
        "string mode,",
        "Expected one of: a, a+, r, r+, w, w+.",
    ] {
        assert!(
            fs_functions.contains(expected),
            "generated baml.fs surface omitted `{expected}`"
        );
    }
    assert!(glob.contains("public bool Matches("));
    assert!(!file.contains("public global::Baml.BamlHandle Handle"));
    assert!(!file.contains("private readonly global::Baml.BamlHandle"));

    let resource_surfaces = [
        (
            "Fs/File.g.cs",
            vec![
                " Text(",
                " TextAsync(",
                " Bytes(",
                " BytesAsync(",
                " Read(",
                " ReadAsync(",
                " ReadBytes(",
                " ReadBytesAsync(",
                " Close(",
                " CloseAsync(",
                " SeekFrom(",
                " SeekFromAsync(",
                " Write(",
                " WriteAsync(",
                " WriteBytes(",
                " WriteBytesAsync(",
            ],
        ),
        (
            "Glob/Glob.g.cs",
            vec![" Scan(", " ScanAsync(", " Matches(", " MatchesAsync("],
        ),
        (
            "Http/Response.g.cs",
            vec![
                " StatusCode { get; }",
                " Headers { get; }",
                " Url { get; }",
                " Text(",
                " TextAsync(",
                " Bytes(",
                " BytesAsync(",
                " Ok(",
                " OkAsync(",
                " New(",
                " NewAsync(",
                " NewStreaming(",
                " NewStreamingAsync(",
                " Write(",
                " WriteAsync(",
                " End(",
                " EndAsync(",
            ],
        ),
        (
            "Http/SseStream.g.cs",
            vec![
                " Url { get; }",
                " Next(",
                " NextAsync(",
                " Close(",
                " CloseAsync(",
            ],
        ),
        (
            "Http/Server.g.cs",
            vec![
                " Addr { get; }",
                " Bind(",
                " BindAsync(",
                " Serve(",
                " ServeAsync(",
            ],
        ),
        (
            "Http/TlsConfig.g.cs",
            vec![" AllowTls12 { get; }", " New(", " NewAsync("],
        ),
        (
            "Net/TcpStream.g.cs",
            vec![
                " Connect(",
                " ConnectAsync(",
                " Read(",
                " ReadAsync(",
                " Write(",
                " WriteAsync(",
                " Close(",
                " CloseAsync(",
            ],
        ),
        (
            "Net/TcpListener.g.cs",
            vec![
                " Bind(",
                " BindAsync(",
                " Accept(",
                " AcceptAsync(",
                " Close(",
                " CloseAsync(",
            ],
        ),
        (
            "Net/UdpSocket.g.cs",
            vec![
                " Bind(",
                " BindAsync(",
                " SendTo(",
                " SendToAsync(",
                " RecvFrom(",
                " RecvFromAsync(",
                " Close(",
                " CloseAsync(",
            ],
        ),
        (
            "Spawn/TaskGroup.g.cs",
            vec![
                " New(",
                " NewAsync(",
                " Cancel(",
                " CancelAsync(",
                " SetLimit(",
                " SetLimitAsync(",
                " Limit(",
                " LimitAsync(",
                " Name(",
                " NameAsync(",
                " ActiveCount(",
                " ActiveCountAsync(",
                " QueuedCount(",
                " QueuedCountAsync(",
            ],
        ),
        (
            "Spawn/CancelToken.g.cs",
            vec![
                " New(",
                " NewAsync(",
                " Any(",
                " AnyAsync(",
                " Cancel(",
                " CancelAsync(",
                " IsCancelled(",
                " IsCancelledAsync(",
            ],
        ),
        (
            "Csv/CsvRecord.g.cs",
            vec![
                " Get<T>(",
                " GetAsync<T>(",
                " GetAt<T>(",
                " GetAtAsync<T>(",
                " Fields(",
                " FieldsAsync(",
                " Length(",
                " LengthAsync(",
                " Position(",
                " PositionAsync(",
                " Decode<T>(",
                " DecodeAsync<T>(",
                " ToMap(",
                " ToMapAsync(",
            ],
        ),
        (
            "Csv/CsvReader.g.cs",
            vec![
                " Headers(",
                " HeadersAsync(",
                " Rows<T>(",
                " RowsAsync<T>(",
                " Skipped(",
                " SkippedAsync(",
                " SkippedCount(",
                " SkippedCountAsync(",
                " Position(",
                " PositionAsync(",
                " Close(",
                " CloseAsync(",
            ],
        ),
        (
            // `iter` / `next` come from `implements root.iter.Iterable` /
            // `Iterator`; interface-impl methods are not generated, so the
            // reader handle is the whole surface here.
            "Csv/CsvRows.g.cs",
            vec![" Reader { get; }"],
        ),
        (
            "Csv/CsvWriter.g.cs",
            vec![
                " WriteRecord(",
                " WriteRecordAsync(",
                " WriteRow<T>(",
                " WriteRowAsync<T>(",
                " WriteRows<T>(",
                " WriteRowsAsync<T>(",
                " WriteHeader(",
                " WriteHeaderAsync(",
                " RecordsWritten(",
                " RecordsWrittenAsync(",
                " Text(",
                " TextAsync(",
                " Flush(",
                " FlushAsync(",
                " Close(",
                " CloseAsync(",
            ],
        ),
    ];
    for (relative, expected) in resource_surfaces {
        let source = fs::read_to_string(generated.join(relative))
            .unwrap_or_else(|error| panic!("failed to read generated {relative}: {error}"));
        for common in [
            " : global::System.IDisposable",
            "public bool IsClosed => resource.IsClosed;",
            " Clone() => new(",
            "public void Dispose() => resource.Dispose();",
        ] {
            assert!(
                source.contains(common),
                "generated resource {relative} omitted `{common}`"
            );
        }
        assert!(
            !source.contains("public global::Baml.BamlHandle Handle"),
            "generated resource {relative} exposed its raw handle"
        );
        for member in expected {
            assert!(
                source.contains(member),
                "generated resource {relative} omitted `{member}`"
            );
        }
    }

    let local_id = fs::read_to_string(
        fixture
            .join("baml_sdk")
            .join("Boundary")
            .join("LocalId.g.cs"),
    )
    .expect("failed to read generated boundary.LocalId resource surface");
    for expected in [
        " : global::System.IDisposable",
        " Capture(",
        " CaptureAsync(",
        " Clone() => new(",
    ] {
        assert!(local_id.contains(expected));
    }

    let function_surfaces = [
        (
            "Fs/Functions.g.cs",
            vec![
                " Open(",
                " OpenAsync(",
                " Exists(",
                " ExistsAsync(",
                " Remove(",
                " RemoveAsync(",
                " Size(",
                " SizeAsync(",
                " Read(",
                " ReadAsync(",
                " Write(",
                " WriteAsync(",
                " WriteBytes(",
                " WriteBytesAsync(",
                " ReadDir(",
                " ReadDirAsync(",
                " Mkdir(",
                " MkdirAsync(",
                " RemoveDir(",
                " RemoveDirAsync(",
                " RemoveDirAll(",
                " RemoveDirAllAsync(",
            ],
        ),
        ("Glob/Functions.g.cs", vec![" New(", " NewAsync("]),
        (
            "Http/Functions.g.cs",
            vec![
                " Fetch(",
                " FetchAsync(",
                " Send(",
                " SendAsync(",
                " FetchSse(",
                " FetchSseAsync(",
            ],
        ),
        (
            "Csv/Functions.g.cs",
            vec![
                " Reader(",
                " ReaderAsync(",
                " Open(",
                " OpenAsync(",
                " Read<T>(",
                " ReadAsync<T>(",
                " Parse(",
                " ParseAsync(",
                " Decode<T>(",
                " DecodeAsync<T>(",
                " DecodeOptional<T>(",
                " DecodeOptionalAsync<T>(",
                " DecodeOne<T>(",
                " DecodeOneAsync<T>(",
                " Writer(",
                " WriterAsync(",
                " Create(",
                " CreateAsync(",
                " Buffer(",
                " BufferAsync(",
                " Write<T>(",
                " WriteAsync<T>(",
                " Stringify<T>(",
                " StringifyAsync<T>(",
                " StringifyRecords(",
                " StringifyRecordsAsync(",
                " ToMarkdown<T>(",
                " ToMarkdownAsync<T>(",
                " ToMarkdownRecords(",
                " ToMarkdownRecordsAsync(",
            ],
        ),
    ];
    for (relative, expected) in function_surfaces {
        let source = fs::read_to_string(generated.join(relative))
            .unwrap_or_else(|error| panic!("failed to read generated {relative}: {error}"));
        for member in expected {
            assert!(
                source.contains(member),
                "generated standard-library functions {relative} omitted `{member}`"
            );
        }
    }

    let boundary_functions = fs::read_to_string(
        fixture
            .join("baml_sdk")
            .join("Boundary")
            .join("Functions.g.cs"),
    )
    .expect("failed to read generated boundary function surface");
    assert!(boundary_functions.contains(" Id("));
    assert!(boundary_functions.contains(" IdAsync("));

    let request = fs::read_to_string(generated.join("Http").join("Request.g.cs"))
        .expect("failed to read generated baml.http.Request structural surface");
    for field in [
        " Method { get; init; }",
        " Url { get; init; }",
        " Headers { get; init; }",
        " Body { get; init; }",
    ] {
        assert!(
            request.contains(field),
            "generated Request omitted `{field}`"
        );
    }

    let duration = fs::read_to_string(generated.join("Time").join("Duration.g.cs"))
        .expect("failed to read generated baml.time.Duration surface");
    for member in [
        " Nanoseconds { get; init; }",
        " Abs(",
        " AbsAsync(",
        " FromNanoseconds(",
        " FromNanosecondsAsync(",
        " FromMicroseconds(",
        " FromMicrosecondsAsync(",
        " FromMilliseconds(",
        " FromMillisecondsAsync(",
        " FromSeconds(",
        " FromSecondsAsync(",
        " FromMinutes(",
        " FromMinutesAsync(",
        " FromHours(",
        " FromHoursAsync(",
        " ToNanoseconds(",
        " ToNanosecondsAsync(",
        " ToMicroseconds(",
        " ToMicrosecondsAsync(",
        " ToMilliseconds(",
        " ToMillisecondsAsync(",
        " ToSeconds(",
        " ToSecondsAsync(",
        " ToMinutes(",
        " ToMinutesAsync(",
        " ToHours(",
        " ToHoursAsync(",
    ] {
        assert!(
            duration.contains(member),
            "generated Duration omitted `{member}`"
        );
    }

    let done = fs::read_to_string(generated.join("Iter").join("Done.g.cs"))
        .expect("failed to read generated baml.iter.Done surface");
    assert!(done.contains("public sealed partial class Done"));
}
