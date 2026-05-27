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

// `File`/`Response` instance methods bodied with `$rust_io_function`
// lower to SysOps, which the host can't invoke as entry points
// (`call_function_sync("baml.fs.File.read", …)` → "not invokable as an
// entry point"). These thin wrappers invoke the method inside a BAML
// body — where sysop dispatch works — and take the handle as a normal
// parameter, so Python can drive a File/Response across multiple calls.
// Passing the same handle into successive wrapper calls is what proves
// the engine-side cursor state survives the FFI round trip.
function file_read(f: baml.fs.File, n: int) -> string {
  f.read(n)
}

function file_text(f: baml.fs.File) -> string {
  f.text()
}

function file_seek(f: baml.fs.File, whence: "start" | "current" | "end", offset: int) -> int {
  f.seek_from(whence, offset)
}

function file_close(f: baml.fs.File) -> null {
  f.close()
}

// HTTP Response body read — also a SysOp, so wrapped the same way.
function response_text(r: baml.http.Response) -> string {
  r.text()
}
