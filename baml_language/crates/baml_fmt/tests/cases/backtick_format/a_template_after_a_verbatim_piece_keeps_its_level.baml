function f(c: bool, name: string) -> string {
    let s = `
        ${if c}yes${endif}
        a
    ` + `
        second
    `;
    let t = /* multi
    line */ `
        tpl
    `;
    let u = "multi
line" + `
    tpl
`;
    let v = f(`
        ${if c}yes${endif}
    `) + f(`
        hugged
    `);
    s
}
