interface Read {
    /// Read some bytes from the source.
    /// When the end of the source has already been reached, returns `null`.
    ///
    /// The `limit` parameter specifies the maximum number of bytes to read.
    ///
    /// This is the only method that implementors are required to provide.
    function read(self, limit: int) -> uint8array? throws root.errors.Io

    /// Reads all remaining bytes from the source until EOF is reached.
    function read_to_end(self) -> uint8array throws root.errors.Io {
        let buf = b"";
        while let chunk: uint8array = self.read(int.max_value()) {
            buf = buf.concat(chunk);
        }
        buf
    }

    /// Reads all remaining bytes as a UTF-8 string until EOF is reached.
    /// If the source is not valid UTF-8, an error is thrown.
    function read_to_string(self) -> string throws root.errors.Io | root.errors.ParseError {
        let buf = self.read_to_end();
        string.from_utf8(buf) catch (_) {
            _ => throw root.errors.ParseError { message: "invalid utf-8" },
        }
    }
}
