interface Write {
    /// Attempt one write of `data` and return the number of bytes accepted.
    /// A partial write is successful.
    function write_some(
        self,
        data: uint8array,
    ) -> int throws root.errors.Io

    /// Write all of `data` to the destination.
    /// Strings are encoded as UTF-8.
    /// Returns the number of bytes written.
    ///
    /// Like Rust's `write_all`, this does not flush: it is a loop over
    /// `write_some`, and pushing buffered bytes onward stays the caller's
    /// separate `flush`. Implementors are expected to make `write_some` report
    /// its own failures rather than defer them.
    function write(
        self,
        data: string | uint8array,
    ) -> int throws root.errors.Io {
        let remaining = match (data) {
            let text: string => text.to_utf8(),
            let bytes: uint8array => bytes,
        };
        let total = remaining.length();

        while (remaining.length() > 0) {
            let written = self.write_some(remaining);
            if (written <= 0 || written > remaining.length()) {
                throw root.errors.Io {
                    message: "writer made invalid progress",
                };
            }
            remaining = remaining.slice(written, remaining.length());
        }

        total
    }

    /// Push data buffered by this writer toward its destination.
    function flush(self) -> void throws root.errors.Io
}
