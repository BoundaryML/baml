function D() -> string {
    let x = `
        a ${
            foo()
        } b
    `;
    x
}
