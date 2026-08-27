interface Read {
    /// Read up to `limit` bytes from the source.
    /// Returns `null` after EOF. A non-null result is never empty.
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
