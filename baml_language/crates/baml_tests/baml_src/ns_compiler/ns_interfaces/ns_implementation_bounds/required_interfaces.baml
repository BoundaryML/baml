test "required interface checks symbolic implementation bounds" {
    root.compiler.AssertCompiles({
        "case.baml": `
            interface Marker {}
            interface Parent {}
            interface Child requires Parent {}
            class Box<T> { value T }
            implements<T extends Marker> Parent for Box<T> {}
            implements<T extends Marker> Child for Box<T> {}
        `,
    })
    let diagnostics = root.compiler.CompileDiagnostics({
        "case.baml": `
            interface Marker {}
            interface Parent {}
            interface Child requires Parent {}
            class Box<T> { value T }
            implements<T extends Marker> Parent for Box<T> {}
            implements<T> Child for Box<T> {}
        `,
    })
    assert.equal(diagnostics.length(), 1)
    assert.equal(diagnostics[0].code, "E0125")
}
