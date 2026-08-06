// builtin media statics resolve through the receiver-class correspondence
function med_image() -> image throws never {
    let img = image.from_url("https://example.com/a.png", "image/png")
     // ^^^ image
    img
}
function med_pdf() -> pdf throws never {
    let doc = pdf.from_base64("aGk=", "application/pdf")
     // ^^^ pdf
    doc
}
