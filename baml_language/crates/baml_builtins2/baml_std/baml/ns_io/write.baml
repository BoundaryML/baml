interface Write {
    /// Write some bytes to the destination.
    ///
    /// Returns the number of bytes successfully written.
    function write(self, data: uint8array) -> int throws root.errors.Io

    /// Flush any buffered data to the destination.
    function flush(self) -> void throws root.errors.Io
}
