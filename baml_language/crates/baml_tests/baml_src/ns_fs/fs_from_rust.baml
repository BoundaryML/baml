// fs error handling tests. fs sysop errors unwind through the VM exception
// machinery and are catchable as baml.errors.* instances.
//
// Error types (from crates/sys_native/src/io_impls.rs):
//   open/read/write/remove/remove_dir/remove_dir_all/read_dir/mkdir failures
//     -> baml.errors.Io
//   file.read(negative n)          -> baml.errors.InvalidArgument
//   operations on a closed handle  -> baml.errors.InvalidArgument ("File is closed")
//   seek_from("start", negative)   -> baml.errors.InvalidArgument
//
// HERMETICITY: same idiom as fs.baml — every test builds a unique path via
// fs_tmp(name) (defined in fs.baml, same namespace) and cleans up any files or
// directories it creates.
//
// Not expressible in BAML:
//   fs_file_invalid_mode              — mode "x" is not in the string-literal
//                                       union (compile-time type error);
//                                       requires #[should_panic].
//   fs_remove_on_symlink_to_dir_removes_link — `baml.fs.symlink` can create the
//                                       link, but the assertion needs
//                                       symlink_metadata (BAML resolves through
//                                       links), and Windows CI cannot create
//                                       one; stays Unix-only in Rust.
//   fs_read_dir_reports_symlink_flag  — same.

// ============================================================================
// open: nonexistent path errors
// ============================================================================

function fs_open_nonexistent_file_fn() -> bool {
    let path = fs_tmp("open_nonexistent") + "/nonexistent.txt";
    {
        baml.fs.open(path, "r").text();
        false
    } catch (e) {
        baml.errors.Io => true
    }
}

test "fs_open_nonexistent_file" {
    assert.is_true(fs_open_nonexistent_file_fn())
}

// ============================================================================
// file.write on a read-only ("r") handle errors with Io "Failed to write:"
// ============================================================================

function fs_file_write_on_readonly_errors_fn() -> bool {
    let path = fs_tmp("write_readonly.txt");
    baml.fs.write(path, "content");
    let file = baml.fs.open(path, "r");
    let msg = {
        file.write("should fail");
        "no error thrown"
    } catch (e) {
        baml.errors.Io => e.message
    };
    baml.fs.remove(path);
    msg.starts_with("Failed to write:")
}

test "fs_file_write_on_readonly_errors" {
    assert.is_true(fs_file_write_on_readonly_errors_fn())
}

// ============================================================================
// remove: nonexistent path errors
// ============================================================================

function fs_remove_nonexistent_errors_fn() -> bool {
    let path = fs_tmp("remove_nonexistent") + "/nope.txt";
    {
        baml.fs.remove(path);
        false
    } catch (e) {
        baml.errors.Io => true
    }
}

test "fs_remove_nonexistent_errors" {
    assert.is_true(fs_remove_nonexistent_errors_fn())
}

// ============================================================================
// remove on a directory: Io error that points at remove_dir / remove_dir_all,
// and the directory survives the failed removal
// ============================================================================

function fs_remove_on_directory_points_at_remove_dir_fn() -> bool {
    let dir = fs_tmp("remove_on_dir");
    baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true });
    let msg = {
        baml.fs.remove(dir);
        "no error thrown"
    } catch (e) {
        baml.errors.Io => e.message
    };
    // The directory must still be there — the failed remove left it untouched.
    let still_there = baml.fs.exists(dir);
    baml.fs.remove_dir(dir);
    // Check the full phrase: "remove_dir" alone is a substring of
    // "remove_dir_all" and would be a vacuous assertion.
    msg.includes("it is a directory")
        && msg.includes("baml.fs.remove_dir or baml.fs.remove_dir_all")
        && still_there
}

test "fs_remove_on_directory_points_at_remove_dir" {
    assert.is_true(fs_remove_on_directory_points_at_remove_dir_fn())
}

// ============================================================================
// remove_dir refuses a non-empty directory; contents survive
// ============================================================================

function fs_remove_dir_on_nonempty_errors_fn() -> bool {
    let dir = fs_tmp("remove_dir_nonempty");
    baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true });
    baml.fs.write(dir + "/file.txt", "x");
    let threw = {
        baml.fs.remove_dir(dir);
        false
    } catch (e) {
        baml.errors.Io => true
    };
    // The directory and its contents survive the failed removal.
    let survived = baml.fs.exists(dir + "/file.txt");
    baml.fs.remove_dir_all(dir);
    threw && survived
}

test "fs_remove_dir_on_nonempty_errors" {
    assert.is_true(fs_remove_dir_on_nonempty_errors_fn())
}

// ============================================================================
// remove_dir is NOT idempotent: a missing path errors
// ============================================================================

function fs_remove_dir_on_missing_errors_fn() -> bool {
    let dir = fs_tmp("remove_dir_missing");
    {
        baml.fs.remove_dir(dir);
        false
    } catch (e) {
        baml.errors.Io => true
    }
}

test "fs_remove_dir_on_missing_errors" {
    assert.is_true(fs_remove_dir_on_missing_errors_fn())
}

// ============================================================================
// remove_dir on a regular file errors; the file survives
// ============================================================================

function fs_remove_dir_on_file_errors_fn() -> bool {
    let path = fs_tmp("remove_dir_on_file.txt");
    baml.fs.write(path, "x");
    let threw = {
        baml.fs.remove_dir(path);
        false
    } catch (e) {
        baml.errors.Io => true
    };
    let survived = baml.fs.exists(path);
    baml.fs.remove(path);
    threw && survived
}

test "fs_remove_dir_on_file_errors" {
    assert.is_true(fs_remove_dir_on_file_errors_fn())
}

// ============================================================================
// remove_dir_all on a regular file errors; the file survives (no data loss)
// ============================================================================

function fs_remove_dir_all_on_file_errors_fn() -> bool {
    let path = fs_tmp("remove_dir_all_on_file.txt");
    baml.fs.write(path, "x");
    let threw = {
        baml.fs.remove_dir_all(path);
        false
    } catch (e) {
        baml.errors.Io => true
    };
    let survived = baml.fs.exists(path);
    baml.fs.remove(path);
    threw && survived
}

test "fs_remove_dir_all_on_file_errors" {
    assert.is_true(fs_remove_dir_all_on_file_errors_fn())
}

// ============================================================================
// file.read with a negative length errors (Io)
// ============================================================================

// A non-positive read limit asks for no bytes, which is an empty read rather
// than an error: `null` is reserved for end-of-file, so returning it here would
// make "you asked for nothing" indistinguishable from "there is nothing left".
function fs_file_read_negative_n_reads_empty_fn() -> uint8array? {
    let path = fs_tmp("read_negative_n.txt");
    baml.fs.write(path, "abc");
    let f = baml.fs.open(path, "r");
    defer { baml.fs.remove(path) }
    f.read(-1)
}

test "fs_file_read_negative_n_reads_empty" {
    assert.equal(fs_file_read_negative_n_reads_empty_fn(), b"")
}

function fs_file_read_zero_reads_empty_fn() -> uint8array? {
    let path = fs_tmp("read_zero.txt");
    baml.fs.write(path, "abc");
    let f = baml.fs.open(path, "r");
    defer { baml.fs.remove(path) }
    f.read(0)
}

test "fs_file_read_zero_reads_empty" {
    assert.equal(fs_file_read_zero_reads_empty_fn(), b"")
}

// ============================================================================
// close invalidates the handle: reading after close errors ("File is closed")
// ============================================================================

function fs_file_close_invalidates_handle_fn() -> bool {
    let path = fs_tmp("close_invalidates.txt");
    baml.fs.write(path, "content");
    let f = baml.fs.open(path, "r");
    f.close();
    defer { baml.fs.remove(path) }
    {
        f.text();
        false
    } catch (e) {
        baml.errors.Io => true
    }
}

test "fs_file_close_invalidates_handle" {
    assert.is_true(fs_file_close_invalidates_handle_fn())
}

// ============================================================================
// baml.fs.write to a directory errors, and the wrapper releases the handle so
// the directory can be removed afterward
// ============================================================================

function fs_write_wrapper_closes_on_error_fn() -> bool {
    let dir = fs_tmp("write_wrapper_dir");
    baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true });
    let threw = {
        baml.fs.write(dir, "oops");
        false
    } catch (e) {
        baml.errors.Io => true
    };
    // If the wrapper leaked the handle, removing the dir on Windows would fail
    // with a sharing violation; on Unix it's a soft check.
    baml.fs.remove_dir(dir);
    threw && !baml.fs.exists(dir)
}

test "fs_write_wrapper_closes_on_error" {
    assert.is_true(fs_write_wrapper_closes_on_error_fn())
}

// ============================================================================
// baml.fs.read on a nonexistent path errors
// ============================================================================

function fs_read_nonexistent_errors_fn() -> bool {
    let path = fs_tmp("read_nonexistent") + "/missing.txt";
    {
        baml.fs.read(path);
        false
    } catch (e) {
        baml.errors.Io => true
    }
}

test "fs_read_nonexistent_errors" {
    assert.is_true(fs_read_nonexistent_errors_fn())
}

// ============================================================================
// seek_from("start", negative) errors (InvalidArgument)
// ============================================================================

function fs_file_seek_negative_errors_fn() -> bool {
    let path = fs_tmp("seek_negative.txt");
    baml.fs.write(path, "content");
    let f = baml.fs.open(path, "r+");
    let threw = {
        f.seek_from("start", -1);
        false
    } catch (e) {
        baml.errors.InvalidArgument => true
    };
    baml.fs.remove(path);
    threw
}

test "fs_file_seek_negative_errors" {
    assert.is_true(fs_file_seek_negative_errors_fn())
}

// ============================================================================
// read_dir on a nonexistent path errors
// ============================================================================

function fs_read_dir_nonexistent_errors_fn() -> bool {
    let dir = fs_tmp("read_dir_nonexistent") + "/no_such_dir";
    {
        baml.fs.read_dir(dir);
        false
    } catch (e) {
        baml.errors.Io => true
    }
}

test "fs_read_dir_nonexistent_errors" {
    assert.is_true(fs_read_dir_nonexistent_errors_fn())
}

// ============================================================================
// read_dir on a path that exists but is a regular file errors — callers must
// be able to distinguish "empty dir" from "you pointed at a file"
// ============================================================================

function fs_read_dir_on_file_errors_fn() -> bool {
    let path = fs_tmp("read_dir_on_file.txt");
    baml.fs.write(path, "x");
    let threw = {
        baml.fs.read_dir(path);
        false
    } catch (e) {
        baml.errors.Io => true
    };
    baml.fs.remove(path);
    threw
}

test "fs_read_dir_on_file_errors" {
    assert.is_true(fs_read_dir_on_file_errors_fn())
}

// ============================================================================
// mkdir non-recursive errors when the parent is missing
// ============================================================================

function fs_mkdir_non_recursive_errors_when_parent_missing_fn() -> bool {
    let dir = fs_tmp("mkdir_no_parent") + "/no/parent";
    {
        baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: false });
        false
    } catch (e) {
        baml.errors.Io => true
    }
}

test "fs_mkdir_non_recursive_errors_when_parent_missing" {
    assert.is_true(fs_mkdir_non_recursive_errors_when_parent_missing_fn())
}

// ============================================================================
// mkdir non-recursive errors when the directory already exists
// ============================================================================

function fs_mkdir_non_recursive_errors_when_dir_exists_fn() -> bool {
    let dir = fs_tmp("mkdir_exists");
    baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true });
    let threw = {
        baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: false });
        false
    } catch (e) {
        baml.errors.Io => true
    };
    baml.fs.remove_dir(dir);
    threw
}

test "fs_mkdir_non_recursive_errors_when_dir_exists" {
    assert.is_true(fs_mkdir_non_recursive_errors_when_dir_exists_fn())
}

// ============================================================================
// mkdir recursive errors when the leaf path exists as a regular file —
// idempotency only applies when the leaf is already a directory
// ============================================================================

function fs_mkdir_recursive_errors_when_leaf_is_file_fn() -> bool {
    let path = fs_tmp("mkdir_leaf.txt");
    baml.fs.write(path, "im a file");
    let threw = {
        baml.fs.mkdir(path, baml.fs.MkdirOptions { recursive: true });
        false
    } catch (e) {
        baml.errors.Io => true
    };
    baml.fs.remove(path);
    threw
}

test "fs_mkdir_recursive_errors_when_leaf_is_file" {
    assert.is_true(fs_mkdir_recursive_errors_when_leaf_is_file_fn())
}
