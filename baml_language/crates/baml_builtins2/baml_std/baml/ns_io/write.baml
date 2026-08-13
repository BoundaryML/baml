interface Write {
    /// Write some bytes to the destination.
    ///
    /// Returns the number of bytes successfully written.
    function write(self, data: uint8array) -> int throws root.errors.Io

    /// Flush any buffered data to the destination.
    function flush(self) -> void throws root.errors.Io

    /// Keep writing data until all bytes have been written.
    function write_all(self, data: uint8array) -> void throws root.errors.Io {
        while (data.length() > 0) {
            let written = self.write(data);
            data = data.slice(written, data.length());
        }
    }
}
