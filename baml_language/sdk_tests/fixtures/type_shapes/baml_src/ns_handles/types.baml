// Handle-backed stdlib types returned to Python (35-series follow-up).
//
// Each wrapper returns a value that carries an engine-minted handle —
// `baml.http.Response._body`, the media `_data`, and `baml.fs.File`'s
// internal handle. The plain Python host can't construct these (there's
// no way to mint a handle host-side; see 35b "non-bug skips"), so a BAML
// function has to produce them. Once Python holds one, it can call the
// type's instance methods, which dispatch back across the FFI with the
// handle as `self`. The `roundtrip_tests/test_handles.py` suite drives
// these against a localhost HTTP server / a temp file so there's no
// external dependency.

// HTTP: send a request and hand the Response back so Python can read
// `status_code`, call `ok()`, `text()`, `bytes()`.
function http_get(url: string) -> baml.http.Response {
  baml.http.send(baml.http.Request {
    method: "GET",
    url: url,
    headers: {},
    body: "",
  })
}

// Media: construct an image from a base64 payload and return it, so
// Python can call `base64()` / `mime_type()` on the handle.
function image_from_base64(data: string, mime: string?) -> image {
  image.from_base64(data, mime)
}

// Filesystem: open a file for reading and return the File handle. The
// key property under test is that the handle carries engine-side cursor
// state across separate instance-method calls from Python (e.g. two
// successive `read(n)` calls advance the cursor).
function open_file_read(path: string) -> baml.fs.File {
  baml.fs.open(path, "r")
}
