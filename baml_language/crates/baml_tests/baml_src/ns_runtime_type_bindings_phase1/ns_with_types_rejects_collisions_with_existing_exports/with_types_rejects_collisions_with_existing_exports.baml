type ExistingAlias = string;

function ExistingFunction() -> null {
    null
}

function main() -> string {
    let dynamic_t = reflect.class.new("Dynamic", { "value": reflect.Type.of<string>() });
    let base = reflect.Package.compile(
        {
            "base.baml": `
                type ExistingAlias = string
                function ExistingFunction() -> null { null }
            `,
        },
    );
    let alias_collision = base.with_types({ "ExistingAlias": dynamic_t }) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code
    };
    let function_collision = base.with_types({ "ExistingFunction": dynamic_t }) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code
    };
    let first_view = base.with_types({ "Mounted": dynamic_t });
    let view_collision = first_view.with_types({ "Mounted": dynamic_t }) catch (e) {
        reflect.errors.CompilationError => e.diagnostics[0].code
    };
    let a = if (alias_collision is string) {
        alias_collision
    } else {
        "alias collision accepted"
    };
    let f = if (function_collision is string) {
        function_collision
    } else {
        "function collision accepted"
    };
    let v = if (view_collision is string) {
        view_collision
    } else {
        "view collision accepted"
    };
    a + "|" + f + "|" + v
}

test "with_types_rejects_collisions_with_existing_exports" {
    assert.equal(main(), "E0011|E0011|E0011")
}
