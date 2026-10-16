function main() -> string {
    let inner = reflect.Class.new("MountedInner", { "value": string.type() });
    let outer = reflect.Class.new("MountedOuter", { "inner": inner.as_type() });
    let app = reflect.Package.compile({ "empty.baml": "" }).with_types({ "OuterAlias": outer });
    let s = reflect.Session.new(packages = { "app": app });
    s.eval<string>(
        `
            let outer = app.OuterAlias {
              inner: app.MountedInner { value: "visible" },
            }
            outer.inner.value
        `,
    )
}

test "session_export_alias_preserves_nested_mounted_field_types" {
    assert.equal(main(), "visible")
}
