// Unified tests for filesystem operations.
//
// Converted from crates/baml_tests/tests/fs.rs. Each Rust test becomes a
// top-level `function` (to avoid the test-block local-boxing VM bug) + a
// `test` block that calls it.
//
// HERMETICITY: every test uses a unique path under /tmp. File-creating tests
// remove their files via baml.fs.remove, and the remove_dir/remove_dir_all
// tests clean up the directories they make. The older directory-creating tests
// (mkdir/read_dir) do NOT delete their dirs; they rely on unique paths +
// idempotent mkdir(recursive:true) so the suite stays safe to re-run and safe
// under the parallel .baml runner.
//
// Tests left in Rust (not converted):
//
//   Should-error tests (cannot catch ExternalOpFailed in BAML — fs op failures
//   are fatal engine errors, not BAML-level catchable throws):
//     fs_open_nonexistent_file
//     fs_file_write_on_readonly_errors
//     fs_remove_nonexistent_errors
//     fs_file_read_negative_n_errors
//     fs_file_close_invalidates_handle
//     fs_write_wrapper_closes_on_error
//     fs_read_nonexistent_errors
//     fs_file_seek_negative_errors
//     fs_read_dir_nonexistent_errors
//     fs_read_dir_on_file_errors
//     fs_mkdir_non_recursive_errors_when_parent_missing
//     fs_mkdir_non_recursive_errors_when_dir_exists
//     fs_mkdir_recursive_errors_when_leaf_is_file
//
//   Other non-convertible tests:
//     fs_file_invalid_mode        — tests a compile-time type error ("x" is not
//                                   in the mode union); the Rust test uses
//                                   #[should_panic] to catch a compile-time panic.
//                                   No BAML equivalent.
//     fs_read_dir_reports_symlink_flag — `baml.fs.symlink` could create the
//                                   link now, but creating one on Windows needs
//                                   privileges CI does not grant, so the test
//                                   stays Unix-only in Rust.

// ============================================================================
// open + read (text)
// ============================================================================

function fs_tmp(name: string) -> string {
    `/tmp/baml_fs_${name}_${baml.time.Instant.now().to_timestamp_milliseconds()}_${int.random(0, 1000000000)}`
}

function fs_open_and_read_fn() -> string {
    let path = fs_tmp("open_and_read.txt");
    baml.fs.write(path, "Hello from BAML!");
    let file = baml.fs.open(path, "r");
    let result = file.text();
    baml.fs.remove(path);
    result
}

test "fs_open_and_read" {
    assert.equal(fs_open_and_read_fn(), "Hello from BAML!")
}

// ============================================================================
// open + read (bytes) — length-only since we can't express raw byte literals
// ============================================================================

function fs_open_and_read_bytes_fn() -> bool {
    let path = fs_tmp("open_and_read_bytes.txt");
    baml.fs.write(path, "Hello from BAML!");
    let file = baml.fs.open(path, "r");
    let data = file.bytes();
    baml.fs.remove(path);
    // "Hello from BAML!" is 16 bytes
    data.length() == 16
}

test "fs_open_and_read_bytes" {
    assert.is_true(fs_open_and_read_bytes_fn())
}

// ============================================================================
// write string — returns byte count
// ============================================================================

function fs_write_string_fn() -> bool {
    let path = fs_tmp("write_string.txt");
    let n = baml.fs.write(path, "Hello, world!");
    let readback = baml.fs.read(path);
    baml.fs.remove(path);
    n == 13 && readback == "Hello, world!"
}

test "fs_write_string" {
    assert.is_true(fs_write_string_fn())
}

// ============================================================================
// write_bytes — binary round-trip (byte count + length match)
// ============================================================================

function fs_write_bytes_fn() -> bool {
    let src = fs_tmp("write_bytes_src.bin");
    let dst = fs_tmp("write_bytes_dst.bin");
    baml.fs.write(src, "binary data");
    let data = baml.fs.open(src, "r").bytes();
    let n = baml.fs.write_bytes(dst, data);
    let readback = baml.fs.open(dst, "r").bytes();
    baml.fs.remove(src);
    baml.fs.remove(dst);
    // "binary data" is 11 bytes; readback should have the same length
    n == 11 && readback.length() == 11
}

test "fs_write_bytes" {
    assert.is_true(fs_write_bytes_fn())
}

// ============================================================================
// write creates parent dirs automatically
// ============================================================================

function fs_write_creates_parent_dirs_fn() -> bool {
    let path = fs_tmp("write_parent") + "/nested/dir/file.txt";
    let n = baml.fs.write(path, "nested content");
    let readback = baml.fs.read(path);
    // Can only remove the file; directories cannot be removed via baml.fs.remove.
    // The path is unique so re-runs are idempotent (write overwrites).
    baml.fs.remove(path);
    n == 14 && readback == "nested content"
}

test "fs_write_creates_parent_dirs" {
    assert.is_true(fs_write_creates_parent_dirs_fn())
}

// ============================================================================
// write overwrites existing file
// ============================================================================

function fs_write_overwrites_existing_fn() -> bool {
    let path = fs_tmp("write_overwrite.txt");
    baml.fs.write(path, "old content");
    let n = baml.fs.write(path, "new content");
    let readback = baml.fs.read(path);
    baml.fs.remove(path);
    n == 11 && readback == "new content"
}

test "fs_write_overwrites_existing" {
    assert.is_true(fs_write_overwrites_existing_fn())
}

// ============================================================================
// roundtrip write then read
// ============================================================================

function fs_roundtrip_write_and_read_fn() -> string {
    let path = fs_tmp("roundtrip.txt");
    baml.fs.write(path, "roundtrip data");
    let f = baml.fs.open(path, "r");
    let result = f.text();
    baml.fs.remove(path);
    result
}

test "fs_roundtrip_write_and_read" {
    assert.equal(fs_roundtrip_write_and_read_fn(), "roundtrip data")
}

// ============================================================================
// seek_from start then read remainder
// ============================================================================

function fs_file_rw_seek_and_read_fn() -> string {
    let path = fs_tmp("seek_and_read.txt");
    baml.fs.write(path, "Hello from BAML!");
    let file = baml.fs.open(path, "r+");
    file.seek_from("start", 6);
    let result = file.text();
    baml.fs.remove(path);
    result
}

test "fs_file_rw_seek_and_read" {
    assert.equal(fs_file_rw_seek_and_read_fn(), "from BAML!")
}

// ============================================================================
// seek, write at offset, seek back, read all
// ============================================================================

function fs_file_rw_write_and_read_back_fn() -> string {
    let path = fs_tmp("rw_write_read.txt");
    baml.fs.write(path, "Hello from BAML!");
    let file = baml.fs.open(path, "r+");
    file.seek_from("start", 6);
    file.write("to Rust!!");
    file.seek_from("start", 0);
    let result = file.text();
    baml.fs.remove(path);
    result
}

test "fs_file_rw_write_and_read_back" {
    assert.equal(fs_file_rw_write_and_read_back_fn(), "Hello to Rust!!!")
}

// ============================================================================
// write_bytes to open file handle
// ============================================================================

function fs_file_rw_write_bytes_fn() -> bool {
    let src = fs_tmp("file_write_bytes_src.bin");
    let dst = fs_tmp("file_write_bytes_dst.bin");
    // source: 2 bytes "AB"
    baml.fs.write(src, "AB");
    // dest: 4 bytes we overwrite at the start
    baml.fs.write(dst, "____");
    let bytes = baml.fs.open(src, "r").bytes();
    let file = baml.fs.open(dst, "r+");
    file.seek_from("start", 0);
    let n = file.write_bytes(bytes);
    baml.fs.remove(src);
    baml.fs.remove(dst);
    // "AB" is 2 bytes
    n == 2
}

test "fs_file_rw_write_bytes" {
    assert.is_true(fs_file_rw_write_bytes_fn())
}

// ============================================================================
// exists returns true
// ============================================================================

function fs_exists_returns_true_fn() -> bool {
    let path = fs_tmp("exists_true.txt");
    baml.fs.write(path, "x");
    let result = baml.fs.exists(path);
    baml.fs.remove(path);
    result
}

test "fs_exists_returns_true" {
    assert.is_true(fs_exists_returns_true_fn())
}

// ============================================================================
// exists returns false
// ============================================================================

function fs_exists_returns_false_fn() -> bool {
    // Use a path that definitely does not exist
    baml.fs.exists(fs_tmp("exists_false_NOPE.txt"))
}

test "fs_exists_returns_false" {
    assert.is_true(!fs_exists_returns_false_fn())
}

// ============================================================================
// remove deletes file
// ============================================================================

function fs_remove_deletes_file_fn() -> bool {
    let path = fs_tmp("remove_deletes.txt");
    baml.fs.write(path, "bye");
    baml.fs.remove(path);
    // after remove, exists should return false
    !baml.fs.exists(path)
}

test "fs_remove_deletes_file" {
    assert.is_true(fs_remove_deletes_file_fn())
}

// ============================================================================
// size returns byte count
// ============================================================================

function fs_size_returns_length_fn() -> int {
    let path = fs_tmp("size.bin");
    baml.fs.write(path, "0123456789");
    let n = baml.fs.size(path);
    baml.fs.remove(path);
    n
}

test "fs_size_returns_length" {
    assert.equal(fs_size_returns_length_fn(), 10)
}

// ============================================================================
// file.read(n) reads up to n bytes as a string
// ============================================================================

function fs_file_read_n_bytes_fn() -> string {
    let path = fs_tmp("read_n.txt");
    baml.fs.write(path, "Hello, world!");
    let f = baml.fs.open(path, "r");
    let result = f.read(5);
    baml.fs.remove(path);
    result
}

test "fs_file_read_n_bytes" {
    assert.equal(fs_file_read_n_bytes_fn(), "Hello")
}

// ============================================================================
// file.read(n) truncates at EOF (doesn't error when n > file size)
// ============================================================================

function fs_file_read_truncates_at_eof_fn() -> string {
    let path = fs_tmp("read_eof.txt");
    baml.fs.write(path, "abc");
    let f = baml.fs.open(path, "r");
    let result = f.read(100);
    baml.fs.remove(path);
    result
}

test "fs_file_read_truncates_at_eof" {
    assert.equal(fs_file_read_truncates_at_eof_fn(), "abc")
}

// ============================================================================
// file.read_bytes(n) reads n raw bytes — verified by length
// ============================================================================

function fs_file_read_bytes_n_fn() -> bool {
    let path = fs_tmp("read_bytes_n.bin");
    baml.fs.write(path, "ABCDE");
    let f = baml.fs.open(path, "r");
    let data = f.read_bytes(3);
    baml.fs.remove(path);
    // should have read exactly 3 bytes
    data.length() == 3
}

test "fs_file_read_bytes_n" {
    assert.is_true(fs_file_read_bytes_n_fn())
}

// ============================================================================
// file.read advances the cursor (sequential reads)
// ============================================================================

function fs_file_read_advances_cursor_fn() -> string {
    let path = fs_tmp("read_advances.txt");
    baml.fs.write(path, "ABCDEFGH");
    let f = baml.fs.open(path, "r");
    f.read(3);
    let result = f.read(3);
    baml.fs.remove(path);
    result
}

test "fs_file_read_advances_cursor" {
    assert.equal(fs_file_read_advances_cursor_fn(), "DEF")
}

// ============================================================================
// open "a" appends to existing file
// ============================================================================

function fs_open_append_creates_and_appends_fn() -> bool {
    let path = fs_tmp("append.txt");
    baml.fs.write(path, "start!");
    let f = baml.fs.open(path, "a");
    let n = f.write("more!");
    let readback = baml.fs.read(path);
    baml.fs.remove(path);
    n == 5 && readback == "start!more!"
}

test "fs_open_append_creates_and_appends" {
    assert.is_true(fs_open_append_creates_and_appends_fn())
}

// ============================================================================
// open "a" creates a missing file
// ============================================================================

function fs_open_append_creates_missing_file_fn() -> bool {
    let path = fs_tmp("append_new.txt");
    // Ensure file doesn't exist from a previous run
    // (if it does exist, this test's logic is still correct since we only
    //  check what we wrote — but let's be safe and overwrite)
    let f = baml.fs.open(path, "w");
    f.write("");
    let f2 = baml.fs.open(path, "a");
    let n = f2.write("hi");
    let readback = baml.fs.read(path);
    baml.fs.remove(path);
    n == 2 && readback == "hi"
}

test "fs_open_append_creates_missing_file" {
    assert.is_true(fs_open_append_creates_missing_file_fn())
}

// ============================================================================
// open "a+" can read after writing (seek back to start)
// ============================================================================

function fs_open_append_plus_can_read_fn() -> string {
    let path = fs_tmp("append_plus.txt");
    baml.fs.write(path, "existing");
    let f = baml.fs.open(path, "a+");
    f.write(" more");
    f.seek_from("start", 0);
    let result = f.text();
    baml.fs.remove(path);
    result
}

test "fs_open_append_plus_can_read" {
    assert.equal(fs_open_append_plus_can_read_fn(), "existing more")
}

// ============================================================================
// close is idempotent (calling close twice returns null both times)
// ============================================================================

function fs_file_close_is_idempotent_fn() -> null {
    let path = fs_tmp("close_idempotent.txt");
    baml.fs.write(path, "x");
    let f = baml.fs.open(path, "r");
    f.close();
    f.close();
    baml.fs.remove(path);
    null
}

test "fs_file_close_is_idempotent" {
    fs_file_close_is_idempotent_fn()
}

// ============================================================================
// baml.fs.read (shorthand convenience function)
// ============================================================================

function fs_read_returns_contents_fn() -> string {
    let path = fs_tmp("read_shorthand.txt");
    baml.fs.write(path, "Hello from fs.read!");
    let result = baml.fs.read(path);
    baml.fs.remove(path);
    result
}

test "fs_read_returns_contents" {
    assert.equal(fs_read_returns_contents_fn(), "Hello from fs.read!")
}

// ============================================================================
// open "w" truncates existing file
// ============================================================================

function fs_open_w_truncates_existing_fn() -> bool {
    let path = fs_tmp("open_w_truncate.txt");
    baml.fs.write(path, "original content");
    let f = baml.fs.open(path, "w");
    let n = f.write("new");
    let readback = baml.fs.read(path);
    baml.fs.remove(path);
    n == 3 && readback == "new"
}

test "fs_open_w_truncates_existing" {
    assert.is_true(fs_open_w_truncates_existing_fn())
}

// ============================================================================
// open "w" creates a missing file
// ============================================================================

function fs_open_w_creates_missing_file_fn() -> bool {
    let path = fs_tmp("open_w_new.txt");
    let f = baml.fs.open(path, "w");
    let n = f.write("hi");
    let readback = baml.fs.read(path);
    baml.fs.remove(path);
    n == 2 && readback == "hi"
}

test "fs_open_w_creates_missing_file" {
    assert.is_true(fs_open_w_creates_missing_file_fn())
}

// ============================================================================
// open "w+" reads after write (truncates on open, so original content gone)
// ============================================================================

function fs_open_w_plus_reads_after_write_fn() -> string {
    let path = fs_tmp("open_w_plus.txt");
    baml.fs.write(path, "original");
    let f = baml.fs.open(path, "w+");
    f.write("fresh");
    f.seek_from("start", 0);
    let result = f.text();
    baml.fs.remove(path);
    result
}

test "fs_open_w_plus_reads_after_write" {
    assert.equal(fs_open_w_plus_reads_after_write_fn(), "fresh")
}

// ============================================================================
// open "w" creates parent dirs automatically
// ============================================================================

function fs_open_w_creates_parent_dirs_fn() -> bool {
    let path = fs_tmp("open_w_parent") + "/nested/dir/new.txt";
    let f = baml.fs.open(path, "w");
    let n = f.write("nested");
    let readback = baml.fs.read(path);
    // Can only remove the file; intermediate dirs cannot be removed via baml.fs.remove.
    // The path is unique per test so re-runs are idempotent (write overwrites).
    baml.fs.remove(path);
    n == 6 && readback == "nested"
}

test "fs_open_w_creates_parent_dirs" {
    assert.is_true(fs_open_w_creates_parent_dirs_fn())
}

// ============================================================================
// read_dir returns entries (count + name membership)
// ============================================================================

function fs_read_dir_returns_entries_fn() -> bool {
    let dir = fs_tmp("read_dir_entries");
    // idempotent mkdir so 2nd run succeeds
    baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true });
    baml.fs.write(dir + "/a.txt", "aaa");
    baml.fs.write(dir + "/b.txt", "bbb");
    // idempotent subdir creation
    baml.fs.mkdir(dir + "/subdir", baml.fs.MkdirOptions { recursive: true });
    let entries = baml.fs.read_dir(dir);
    // Remove files (can't remove dirs)
    baml.fs.remove(dir + "/a.txt");
    baml.fs.remove(dir + "/b.txt");
    // 3 entries: a.txt, b.txt, subdir
    let count_ok = entries.length() == 3;
    let has_a = entries.some((e: baml.fs.DirEntry) -> bool { e.name == "a.txt" });
    let has_b = entries.some((e: baml.fs.DirEntry) -> bool { e.name == "b.txt" });
    let has_sub = entries.some((e: baml.fs.DirEntry) -> bool { e.name == "subdir" });
    count_ok && has_a && has_b && has_sub
}

test "fs_read_dir_returns_entries" {
    assert.is_true(fs_read_dir_returns_entries_fn())
}

// ============================================================================
// read_dir type flags (is_file / is_dir / is_symlink)
// ============================================================================

function fs_read_dir_type_flags_fn() -> bool {
    let dir = fs_tmp("read_dir_flags");
    baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true });
    baml.fs.write(dir + "/file.txt", "content");
    baml.fs.mkdir(dir + "/subdir", baml.fs.MkdirOptions { recursive: true });
    let entries = baml.fs.read_dir(dir);
    // Remove file (can't remove dirs)
    baml.fs.remove(dir + "/file.txt");
    // Verify file.txt has is_file=true, is_dir=false, is_symlink=false
    let file_ok = entries.some((e: baml.fs.DirEntry) -> bool {
        e.name == "file.txt" && e.is_file && !e.is_dir && !e.is_symlink
    });
    // Verify subdir has is_dir=true, is_file=false, is_symlink=false
    let dir_ok = entries.some((e: baml.fs.DirEntry) -> bool {
        e.name == "subdir" && e.is_dir && !e.is_file && !e.is_symlink
    });
    file_ok && dir_ok
}

test "fs_read_dir_type_flags" {
    assert.is_true(fs_read_dir_type_flags_fn())
}

// ============================================================================
// read_dir on empty directory returns empty array
// ============================================================================

function fs_read_dir_empty_dir_fn() -> bool {
    let dir = fs_tmp("read_dir_empty");
    baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true });
    let entries = baml.fs.read_dir(dir);
    // Can't remove the dir, but it stays empty so re-runs are idempotent
    entries.length() == 0
}

test "fs_read_dir_empty_dir" {
    assert.is_true(fs_read_dir_empty_dir_fn())
}

// ============================================================================
// mkdir creates a new directory (recursive:true for idempotency across re-runs)
// ============================================================================

function fs_mkdir_creates_directory_fn() -> bool {
    let dir = fs_tmp("mkdir_new");
    // recursive:true so re-runs don't fail when the dir already exists
    baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true });
    baml.fs.exists(dir)
}

test "fs_mkdir_creates_directory" {
    assert.is_true(fs_mkdir_creates_directory_fn())
}

// ============================================================================
// mkdir recursive creates all parent directories
// ============================================================================

function fs_mkdir_recursive_creates_parents_fn() -> bool {
    let dir = fs_tmp("mkdir_recursive") + "/a/b/c";
    baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true });
    baml.fs.exists(dir)
}

test "fs_mkdir_recursive_creates_parents" {
    assert.is_true(fs_mkdir_recursive_creates_parents_fn())
}

// ============================================================================
// mkdir recursive is idempotent when directory already exists
// ============================================================================

function fs_mkdir_recursive_is_idempotent_fn() -> bool {
    let dir = fs_tmp("mkdir_idempotent");
    baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true });
    // second call must not throw
    baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true });
    baml.fs.exists(dir)
}

test "fs_mkdir_recursive_is_idempotent" {
    assert.is_true(fs_mkdir_recursive_is_idempotent_fn())
}

// ============================================================================
// remove_dir removes an empty directory
// ============================================================================

function fs_remove_dir_removes_empty_fn() -> bool {
    let dir = fs_tmp("remove_dir_empty");
    baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true });
    baml.fs.remove_dir(dir);
    !baml.fs.exists(dir)
}

test "fs_remove_dir_removes_empty" {
    assert.is_true(fs_remove_dir_removes_empty_fn())
}

// ============================================================================
// remove_dir_all removes a populated directory tree
// ============================================================================

function fs_remove_dir_all_removes_tree_fn() -> bool {
    let dir = fs_tmp("remove_dir_all_tree");
    baml.fs.mkdir(dir + "/sub", baml.fs.MkdirOptions { recursive: true });
    baml.fs.write(dir + "/a.txt", "a");
    baml.fs.write(dir + "/sub/b.txt", "b");
    baml.fs.remove_dir_all(dir);
    !baml.fs.exists(dir)
}

test "fs_remove_dir_all_removes_tree" {
    assert.is_true(fs_remove_dir_all_removes_tree_fn())
}

// ============================================================================
// remove_dir_all is idempotent on a missing path (force: true semantics)
// ============================================================================

function fs_remove_dir_all_idempotent_fn() -> bool {
    let dir = fs_tmp("remove_dir_all_missing");
    // dir was never created; removing it must not throw
    baml.fs.remove_dir_all(dir);
    !baml.fs.exists(dir)
}

test "fs_remove_dir_all_idempotent" {
    assert.is_true(fs_remove_dir_all_idempotent_fn())
}

// ============================================================================
// chmod
// ============================================================================
//
// Only the platform-independent behavior lives here. What `mode` does to a file
// is meaningful on unix and reduces to one read-only bit on Windows, and BAML
// has no way to read a mode back, so the effect is asserted from Rust
// (tests/fs.rs::fs_chmod_sets_the_exact_mode).

function fs_chmod_on_existing_file_fn() -> bool {
    let path = fs_tmp("chmod_ok.txt");
    baml.fs.write(path, "content");
    baml.fs.chmod(path, 0o644);
    let existed = baml.fs.exists(path);
    baml.fs.remove(path);
    existed
}

test "fs_chmod_on_existing_file" {
    assert.is_true(fs_chmod_on_existing_file_fn())
}

// A mode outside `0..=0o7777` is a caller mistake, rejected before the syscall
// the kernel would have silently masked it in.
function fs_chmod_rejects_high_mode_fn() -> bool {
    let path = fs_tmp("chmod_high_mode.txt");
    baml.fs.write(path, "content");
    let threw = {
        baml.fs.chmod(path, 0o10000);
        false
    } catch (e) {
        baml.errors.InvalidArgument => true
    };
    baml.fs.remove(path);
    threw
}

test "fs_chmod_rejects_high_mode" {
    assert.is_true(fs_chmod_rejects_high_mode_fn())
}

function fs_chmod_rejects_negative_mode_fn() -> bool {
    let path = fs_tmp("chmod_negative_mode.txt");
    baml.fs.write(path, "content");
    let threw = {
        baml.fs.chmod(path, 0 - 1);
        false
    } catch (e) {
        baml.errors.InvalidArgument => true
    };
    baml.fs.remove(path);
    threw
}

test "fs_chmod_rejects_negative_mode" {
    assert.is_true(fs_chmod_rejects_negative_mode_fn())
}

// A missing path is an I/O failure on every platform — including Windows, where
// the current attributes are read before they are rewritten.
function fs_chmod_missing_path_errors_fn() -> bool {
    let path = fs_tmp("chmod_missing.txt");
    {
        baml.fs.chmod(path, 0o644);
        false
    } catch (e) {
        baml.errors.Io => true
    }
}

test "fs_chmod_missing_path_errors" {
    assert.is_true(fs_chmod_missing_path_errors_fn())
}

// ============================================================================
// symlink
// ============================================================================
//
// Creating a link needs privileges Windows CI does not grant, so the success
// path is Unix-only and lives in Rust (tests/fs.rs::fs_symlink_*). What is
// asserted here holds even where symlink creation is refused outright: the call
// fails with `Io` and leaves the occupied path alone.

function fs_symlink_onto_existing_path_errors_fn() -> bool {
    let path = fs_tmp("symlink_taken.txt");
    baml.fs.write(path, "keep me");
    let threw = {
        baml.fs.symlink(path, path);
        false
    } catch (e) {
        baml.errors.Io => true
    };
    let intact = baml.fs.read(path) == "keep me";
    baml.fs.remove(path);
    threw && intact
}

test "fs_symlink_onto_existing_path_errors" {
    assert.is_true(fs_symlink_onto_existing_path_errors_fn())
}
