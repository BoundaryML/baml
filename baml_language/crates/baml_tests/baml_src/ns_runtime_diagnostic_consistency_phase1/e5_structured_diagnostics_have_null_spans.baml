function main() -> string {
    let zero_result = reflect.union.new([]) catch (e) {
        reflect.errors.CompilationError { diagnostics } => {
            diagnostics[0].code + ":" + (diagnostics[0].span == null).to_string()
        },
        _ => "wrong-zero-error",
    };
    let zero = if (zero_result is string) {
        zero_result
    } else {
        "zero did not throw"
    };

    let builder = reflect.class.builder("Duplicate");
    builder.field("x", reflect.Type.of<string>());
    let duplicate_result = builder.field("x", reflect.Type.of<int>()) catch (e) {
        reflect.errors.CompilationError { diagnostics } => {
            diagnostics[0].code + ":" + (diagnostics[0].span == null).to_string()
        },
        _ => "wrong-builder-error",
    };
    let duplicate = if (duplicate_result is string) {
        duplicate_result
    } else {
        "duplicate did not throw"
    };

    let package_result = reflect.Package.compile({ "broken.baml": "class {" }) catch (e) {
        reflect.errors.CompilationError { diagnostics } => {
            (diagnostics[0].span != null).to_string()
        },
        _ => "wrong-package-error",
    };
    let package_span = if (package_result is string) {
        package_result
    } else {
        "package did not throw"
    };

    zero + "|" + duplicate + "|" + package_span
}

test "e5_structured_diagnostics_have_null_spans" {
    assert.equal(main(), "E0160:true|E0012:true|true")
}
