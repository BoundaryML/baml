// BAML Builtin: PDF constructors
// Namespace: baml.pdf
//
// Thin wrappers around baml.Media.from_* with kind="pdf".

function from_url(url: string, mime_type: string?) -> pdf {
    baml.Media.from_url("pdf", url, mime_type)
}

function from_base64(base64: string, mime_type: string) -> pdf {
    baml.Media.from_base64("pdf", base64, mime_type)
}

function from_file(file: string, mime_type: string?) -> pdf {
    baml.Media.from_file("pdf", file, mime_type)
}
