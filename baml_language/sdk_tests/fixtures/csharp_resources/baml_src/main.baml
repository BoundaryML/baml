function OpenFile(path: string) -> baml.fs.File {
  baml.fs.open(path, "r")
}

function FetchUrl(url: string) -> baml.http.Response {
  baml.http.fetch(url)
}
