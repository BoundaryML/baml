class ChunkReader {
    data: uint8array,
    cursor: int,
    chunk_size: int,

    implements baml.io.Read {
        function read(self, limit: int) -> uint8array? {
            if (self.cursor >= self.data.length()) {
                return null;
            }
            let count = if (limit < self.chunk_size) {
                limit
            } else {
                self.chunk_size
            };
            let end = if (self.cursor + count < self.data.length()) {
                self.cursor + count
            } else {
                self.data.length()
            };
            let chunk = self.data.slice(self.cursor, end);
            self.cursor = end;
            chunk
        }
    }
}

class ChunkWriter {
    data: uint8array,
    chunk_size: int,

    implements baml.io.Write {
        function write_some(self, data: uint8array) -> int {
            let count = if (data.length() < self.chunk_size) {
                data.length()
            } else {
                self.chunk_size
            };
            self.data = self.data.concat(data.slice(0, count));
            count
        }

        function flush(self) -> void {}
    }
}

class ZeroWriter {
    implements baml.io.Write {
        function write_some(self, data: uint8array) -> int {
            0
        }
        function flush(self) -> void {}
    }
}

class OversizedWriter {
    implements baml.io.Write {
        function write_some(self, data: uint8array) -> int {
            data.length() + 1
        }
        function flush(self) -> void {}
    }
}

test "read bytes consumes chunks through eof" {
    let reader = ChunkReader { data: "abcdef".to_utf8(), cursor: 0, chunk_size: 2 };
    assert.equal(reader.bytes(), "abcdef".to_utf8());
    assert.equal(reader.read(8), null)
}

test "read text decodes strict utf8" {
    let reader = ChunkReader { data: "hello".to_utf8(), cursor: 0, chunk_size: 1 };
    assert.equal(reader.text(), "hello")
}

test "read text rejects invalid utf8" {
    let reader = ChunkReader { data: uint8array.from_array([255]), cursor: 0, chunk_size: 1 };
    assert.is_true(
        {
            reader.text();
            false
        } catch (e) {
            baml.errors.ParseError => true
        },
    )
}

test "write retries partial writes" {
    let writer = ChunkWriter { data: b"", chunk_size: 2 };
    assert.equal(writer.write("hello"), 5);
    assert.equal(writer.data, "hello".to_utf8())
}

test "write rejects zero progress" {
    assert.is_true(
        {
            ZeroWriter {  }.write(b"abc");
            false
        } catch (e) {
            baml.errors.Io => true
        },
    )
}

test "write rejects oversized progress" {
    assert.is_true(
        {
            OversizedWriter {  }.write(b"abc");
            false
        } catch (e) {
            baml.errors.Io => true
        },
    )
}

test "uint8array index_of finds bytes" {
    let data = uint8array.from_array([10, 20, 10]);
    assert.equal(data.index_of(10), 0);
    assert.equal(data.index_of(99), null);
    assert.equal(data.index_of(256), null)
}
