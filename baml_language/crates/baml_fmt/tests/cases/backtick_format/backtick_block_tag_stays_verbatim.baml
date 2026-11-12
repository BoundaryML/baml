function F(xs: string[]) -> string {
    `${for (let x in xs)}- ${x}
${endfor}`
}
