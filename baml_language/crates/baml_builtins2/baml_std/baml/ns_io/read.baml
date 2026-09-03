/// A common interface for reading bytes from a source.
///
/// Implementors are expected to provide a `read` method that attempts to read up to `limit` bytes from the source.
///
/// Based on Rust's [`Read`](https://doc.rust-lang.org/std/io/trait.Read.html) trait
/// and JavaScript's [`ReadableStream`](https://developer.mozilla.org/en-US/docs/Web/API/ReadableStream).
interface Read {
    /// Read up to `limit` bytes from the source.
    ///
    /// Returns `null` after EOF. A non-null result is empty only when `limit`
    /// is non-positive: `null` is reserved for end of input, so a caller that
    /// asks for no bytes gets an empty read back rather than an EOF signal.
    /// Every implementor agrees on this — driving a loop off `read` therefore
    /// requires a positive limit, as `bytes` and `text` below use.
    function read(self, limit: int) -> uint8array? throws root.errors.Io

    /// Consume all remaining bytes through EOF.
    function bytes(self) -> uint8array throws root.errors.Io {
        let result = b"";
        while let chunk: uint8array = self.read(65536) {
            result = result.concat(chunk);
        }
        result
    }

    /// Consume all remaining bytes through EOF and decode strict UTF-8.
    function text(
        self,
    ) -> string throws root.errors.Io | root.errors.ParseError {
        let data = self.bytes();
        string.from_utf8(data) catch (e) {
            root.errors.InvalidArgument => {
                throw root.errors.ParseError {
                    message: "invalid utf-8",
                }
            }
        }
    }
}
