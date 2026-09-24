/// Static class docs
class StaticDoc {
    /// Static field docs
    value: string @description("Static field description"),
}

enum StaticState {
    /// Static enum value docs
    READY,
}

interface DocAnchor {
    value: string,
}

function main() -> string {
    let anchor = reflect.interface.implementation<DocAnchor>().field("value");
    let minted = reflect.class.new(
        "Minted",
        {
            "value": reflect.Type.of<string>().meta(
                description = "Minted field description",
                docstring = "Minted field docs",
            ),
        },
        implementations = [anchor],
    );
    let app = reflect.Package.compile({ "empty.baml": "" }).with_types({ "Minted": minted });
    let compiled = reflect.Package.compile(
        {
            "wrapper.baml": `
                function make_mounted() -> app.Minted {
                  app.Minted { value: "ok" }
                }
            `,
        },
        packages = { "app": app },
    );
    let make_mounted = compiled.get_function<() -> DocAnchor>("root.make_mounted") ?? throw "missing make_mounted";
    let mounted = reflect.Type.of_value(make_mounted());

    `${reflect.Type.of<StaticDoc>().to_baml()}~~${reflect.Type.of<StaticState>().to_baml()}~~${ai.wire.render_output_format(reflect.Type.of<StaticDoc>())}~~${ai.wire.render_output_format(reflect.Type.of<StaticState>())}~~${minted.as_type().to_baml()}~~${ai.wire.render_output_format(minted.as_type())}~~${mounted.to_baml()}~~${ai.wire.render_output_format(mounted)}`
}

test "docstrings_render_for_static_minted_and_mounted_types" {
    let parts = main().split("~~");
    assert.equal(parts.length(), 8);
    assert.is_true(parts[0].starts_with("/// Static class docs\nclass "));
    assert.contains(parts[0], "StaticDoc {");
    assert.contains(parts[0], "  /// Static field docs\n  value string");
    assert.contains(parts[1], "  /// Static enum value docs\n  READY");
    let static_description = parts[2].index_of("/// Static field description") ?? throw "missing static description";
    let static_docstring = parts[2].index_of("/// Static field docs") ?? throw "missing static docstring";
    assert.is_true(static_description < static_docstring);
    assert.contains(parts[2], "/// Static class docs");
    assert.contains(parts[3], "READY: Static enum value docs");
    assert.contains(parts[4], "  /// Minted field docs\n  value string");
    let minted_description = parts[5].index_of("/// Minted field description") ?? throw "missing minted description";
    let minted_docstring = parts[5].index_of("/// Minted field docs") ?? throw "missing minted docstring";
    assert.is_true(minted_description < minted_docstring);
    assert.contains(parts[6], "/// Minted field docs");
    assert.contains(parts[7], "/// Minted field docs")
}
