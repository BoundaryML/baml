// Tests for baml.glob namespace.
//
// HERMETICITY: every scan test creates a unique temp directory under /tmp
// (using baml.time.Instant.now() and int.random as discriminators), writes fixture files with
// baml.fs.write / baml.fs.mkdir, runs the assertions, then removes the
// directory tree with baml.sys.shell("rm -rf …"). All filesystem work is
// done inside top-level functions (not in test-block locals) to avoid the
// VM local-boxing bug: a `let` binding read back in a test block is boxed as
// Any, which causes sys-op calls like `baml.fs.mkdir` to receive `null`
// instead of the string path. Locals inside a top-level `function` do not
// have this problem.
//
// string.from renders the epoch-milliseconds bigint in decimal (e.g. 1234567890n
// → "1234567890"); used to build unique temp dir paths. baml.json.stringify
// cannot take its place here — the `json` type admits `int` but not `bigint`.
//
// Scan failures (broken symlink with throw_error_on_broken_symlink=true,
// unreadable directories) surface as BAML-catchable `baml.errors.Io` throws,
// so the should-error tests use the standard catch pattern. Permissions are
// manipulated with `chmod` via baml.sys.shell; the permission-error test
// probes readability first and trivially passes under root (CI containers),
// where the test premise is invalid.

// ============================================================================
// baml.glob.new + Glob.matches — no filesystem needed
// ============================================================================

function glob_tmp(name: string) -> string {
    `/tmp/baml_glob_${name}_${baml.time.Instant.now().to_timestamp_milliseconds()}_${int.random(0, 1000000000)}`
}

test "glob_matches_basic" {
    let g = baml.glob.new("*.txt");
    assert.is_true(g.matches("hello.txt"))
}

test "glob_matches_no_match" {
    let g = baml.glob.new("*.txt");
    assert.is_true(!g.matches("hello.rs"))
}

test "glob_matches_recursive_wildcard" {
    let g = baml.glob.new("**/*.ts");
    assert.is_true(g.matches("src/index.ts"))
}

test "glob_matches_question_mark" {
    let g = baml.glob.new("file?.txt");
    assert.is_true(g.matches("fileA.txt"))
}

test "glob_matches_question_mark_no_match" {
    let g = baml.glob.new("file?.txt");
    assert.is_true(!g.matches("file.txt"))
}

// [abc] matches a single character from the set.
function glob_matches_bracket_character_class_fn() -> bool[] {
    let g = baml.glob.new("[abc].txt");
    [g.matches("a.txt"), g.matches("b.txt"), g.matches("c.txt"), g.matches("d.txt")]
}

test "glob_matches_bracket_character_class" {
    assert.equal(glob_matches_bracket_character_class_fn(), [true, true, true, false])
}

// [a-c] matches any character in the range.
function glob_matches_bracket_range_fn() -> bool[] {
    let g = baml.glob.new("[a-c].txt");
    [g.matches("a.txt"), g.matches("c.txt"), g.matches("d.txt")]
}

test "glob_matches_bracket_range" {
    assert.equal(glob_matches_bracket_range_fn(), [true, true, false])
}

// [^abc] matches a single character NOT in the set.
function glob_matches_negated_bracket_fn() -> bool[] {
    let g = baml.glob.new("[^abc].txt");
    [g.matches("a.txt"), g.matches("d.txt")]
}

test "glob_matches_negated_bracket" {
    assert.equal(glob_matches_negated_bracket_fn(), [false, true])
}

// *.{ts,tsx} alternation.
function glob_matches_alternation_fn() -> bool[] {
    let g = baml.glob.new("*.{ts,tsx}");
    [g.matches("app.ts"), g.matches("app.tsx"), g.matches("app.js")]
}

test "glob_matches_alternation" {
    assert.equal(glob_matches_alternation_fn(), [true, true, false])
}

// !index.ts — negated pattern matches everything except that file.
function glob_matches_negated_pattern_fn() -> bool[] {
    let g = baml.glob.new("!index.ts");
    [g.matches("main.ts"), g.matches("index.ts")]
}

test "glob_matches_negated_pattern" {
    assert.equal(glob_matches_negated_pattern_fn(), [true, false])
}

// ============================================================================
// baml.glob.new + Glob.scan — hermetic temp dirs
//
// All filesystem work lives in top-level functions to avoid the VM
// local-boxing bug. The helper creates the fixture, scans, cleans up, and
// returns the information needed for assertions.
// ============================================================================

// ─── glob_scan_finds_txt_files ───────────────────────────────────────────────

function scan_finds_txt_files() -> string[] {
    let root = glob_tmp("scan_txt");
    baml.fs.mkdir(root, baml.fs.MkdirOptions { recursive: true });
    let _ = baml.fs.write(root + "/a.txt", "a");
    let _ = baml.fs.write(root + "/b.txt", "b");
    let _ = baml.fs.write(root + "/c.rs", "c");
    let g = baml.glob.new("*.txt");
    let result = g.scan(root);
    let _ = baml.sys.shell("rm -rf " + root, null);
    result
}

test "glob_scan_finds_txt_files_length" {
    assert.equal(scan_finds_txt_files().length(), 2)
}

test "glob_scan_finds_txt_files_has_a" {
    assert.is_true(scan_finds_txt_files().includes("a.txt"))
}

test "glob_scan_finds_txt_files_has_b" {
    assert.is_true(scan_finds_txt_files().includes("b.txt"))
}

// ─── glob_scan_returns_empty_for_no_match ────────────────────────────────────

function scan_returns_empty_for_no_match() -> string[] {
    let root = glob_tmp("scan_empty");
    baml.fs.mkdir(root, baml.fs.MkdirOptions { recursive: true });
    let _ = baml.fs.write(root + "/a.rs", "rust");
    let _ = baml.fs.write(root + "/b.rs", "rust");
    let g = baml.glob.new("*.txt");
    let result = g.scan(root);
    let _ = baml.sys.shell("rm -rf " + root, null);
    result
}

test "glob_scan_returns_empty_for_no_match" {
    assert.equal(scan_returns_empty_for_no_match().length(), 0)
}

// ─── glob_scan_only_files_by_default ─────────────────────────────────────────

// Default scan only returns files, not directories, even if the dir name
// matches the pattern.
function scan_only_files_by_default_checks() -> bool[] {
    let root = glob_tmp("only_files");
    baml.fs.mkdir(root, baml.fs.MkdirOptions { recursive: true });
    let _ = baml.fs.write(root + "/file.txt", "content");
    // Create a directory named dir.txt — should be excluded from *.txt results.
    baml.fs.mkdir(root + "/dir.txt", baml.fs.MkdirOptions { recursive: false });
    let g = baml.glob.new("*.txt");
    let result = g.scan(root);
    let ok = [
        result.length() == 1,
        result.includes("file.txt"),
    ];
    let _ = baml.sys.shell("rm -rf " + root, null);
    ok
}

test "glob_scan_only_files_by_default" {
    assert.equal(scan_only_files_by_default_checks(), [true, true])
}

// ─── glob_scan_options_include_dot_absolute_and_directories ──────────────────

// dot=true, absolute=true, only_files=false: returns dotfiles and dirs. We
// encode all assertions in one bool[] return [length_ok, has_hidden, has_dir,
// has_file] so a single hermetic run (unique temp root) covers the test.
function scan_opts_include_dot_absolute_and_dirs_checks() -> bool[] {
    let root = glob_tmp("scan_opts2");
    baml.fs.mkdir(root, baml.fs.MkdirOptions { recursive: true });
    let _ = baml.fs.write(root + "/file.txt", "content");
    let _ = baml.fs.write(root + "/.hidden.txt", "hidden");
    baml.fs.mkdir(root + "/dir.txt", baml.fs.MkdirOptions { recursive: false });
    let g = baml.glob.new("**/*.txt");
    let result = g.scan(baml.glob.ScanOptions { cwd: root, dot: true, absolute: true, only_files: false });
    let ok = [
        result.length() == 3,
        result.includes(root + "/.hidden.txt"),
        result.includes(root + "/dir.txt"),
        result.includes(root + "/file.txt"),
    ];
    let _ = baml.sys.shell("rm -rf " + root, null);
    ok
}

test "glob_scan_options_include_dot_absolute_and_directories" {
    assert.equal(scan_opts_include_dot_absolute_and_dirs_checks(), [true, true, true, true])
}

// ─── glob_scan_matches_dot_slash_pattern ─────────────────────────────────────

function scan_matches_dot_slash_pattern() -> string[] {
    let root = glob_tmp("dotslash");
    baml.fs.mkdir(root, baml.fs.MkdirOptions { recursive: true });
    let _ = baml.fs.write(root + "/file.txt", "content");
    let g = baml.glob.new("./*.txt");
    let result = g.scan(root);
    let _ = baml.sys.shell("rm -rf " + root, null);
    result
}

test "glob_scan_matches_dot_slash_pattern_length" {
    assert.equal(scan_matches_dot_slash_pattern().length(), 1)
}

test "glob_scan_matches_dot_slash_pattern_has_file" {
    assert.is_true(scan_matches_dot_slash_pattern().includes("file.txt"))
}

// ─── glob_scan_matches_absolute_pattern ──────────────────────────────────────

function scan_matches_absolute_pattern_checks() -> bool[] {
    let root = glob_tmp("abs_pat");
    baml.fs.mkdir(root, baml.fs.MkdirOptions { recursive: true });
    let _ = baml.fs.write(root + "/file.txt", "content");
    let pattern = root + "/**/*.txt";
    let g = baml.glob.new(pattern);
    let result = g.scan(baml.glob.ScanOptions { cwd: root, absolute: true });
    let ok = [
        result.length() == 1,
        result.includes(root + "/file.txt"),
    ];
    let _ = baml.sys.shell("rm -rf " + root, null);
    ok
}

test "glob_scan_matches_absolute_pattern" {
    assert.equal(scan_matches_absolute_pattern_checks(), [true, true])
}

// ─── glob_scan_dot_pattern_matches_only_dot_files ────────────────────────────

// Regression: `.*` must match `.hidden` but NOT `regular.txt`.
function scan_dot_pattern_matches_only_dot_files() -> string[] {
    let root = glob_tmp("dotpat");
    baml.fs.mkdir(root, baml.fs.MkdirOptions { recursive: true });
    let _ = baml.fs.write(root + "/.hidden", "h");
    let _ = baml.fs.write(root + "/regular.txt", "r");
    let g = baml.glob.new(".*");
    let result = g.scan(baml.glob.ScanOptions { cwd: root, dot: true });
    let _ = baml.sys.shell("rm -rf " + root, null);
    result
}

test "glob_scan_dot_pattern_matches_only_dot_files_length" {
    assert.equal(scan_dot_pattern_matches_only_dot_files().length(), 1)
}

test "glob_scan_dot_pattern_matches_only_dot_files_has_hidden" {
    assert.is_true(scan_dot_pattern_matches_only_dot_files().includes(".hidden"))
}

// ─── glob_scan_default_prunes_dot_directories ────────────────────────────────

// dot=false (default) must skip files inside dot directories (e.g. .git/).
function scan_default_prunes_dot_directories_checks() -> bool[] {
    let root = glob_tmp("prune_dot");
    baml.fs.mkdir(root, baml.fs.MkdirOptions { recursive: true });
    let _ = baml.fs.write(root + "/visible.txt", "visible");
    baml.fs.mkdir(root + "/.git", baml.fs.MkdirOptions { recursive: false });
    baml.fs.mkdir(root + "/.git/objects", baml.fs.MkdirOptions { recursive: false });
    let _ = baml.fs.write(root + "/.git/objects/secret.txt", "x");
    let _ = baml.fs.write(root + "/.git/HEAD", "x");
    let g = baml.glob.new("**/*.txt");
    let result = g.scan(root);
    let ok = [
        result.length() == 1,
        result.includes("visible.txt"),
    ];
    let _ = baml.sys.shell("rm -rf " + root, null);
    ok
}

test "glob_scan_default_prunes_dot_directories" {
    assert.equal(scan_default_prunes_dot_directories_checks(), [true, true])
}

// ─── glob_scan_follows_symlinked_directory_when_enabled (unix) ───────────────

// follow_symlinks=true: a symlinked directory is traversed and its contents matched.
// Needs host-created symlink; baml.fs has no native symlink primitive.
function scan_follows_symlinked_directory_checks() -> bool[] {
    let outer_root = glob_tmp("sym_outer");
    let target = glob_tmp("sym_target");
    baml.fs.mkdir(outer_root, baml.fs.MkdirOptions { recursive: true });
    baml.fs.mkdir(target, baml.fs.MkdirOptions { recursive: true });
    let _ = baml.fs.write(target + "/hidden_via_link.txt", "x");
    // Create symlink: outer_root/link_dir -> target
    let _ = baml.sys.shell("ln -s " + target + " " + outer_root + "/link_dir", null);
    let g = baml.glob.new("**/*.txt");
    let result = g.scan(baml.glob.ScanOptions { cwd: outer_root, follow_symlinks: true });
    let ok = [
        result.length() == 1,
        result.includes("link_dir/hidden_via_link.txt"),
    ];
    let _ = baml.sys.shell("rm -rf " + outer_root + " " + target, null);
    ok
}

test "glob_scan_follows_symlinked_directory_when_enabled" {
    assert.equal(scan_follows_symlinked_directory_checks(), [true, true])
}

// ─── glob_scan_skips_broken_symlinks_by_default (unix) ───────────────────────

// follow_symlinks=true + a broken symlink: default behaviour is to skip it
// and still return the real files.
function scan_skips_broken_symlinks_by_default() -> string[] {
    let root = glob_tmp("broken_sym");
    baml.fs.mkdir(root, baml.fs.MkdirOptions { recursive: true });
    let _ = baml.fs.write(root + "/real.txt", "content");
    // Create a dangling symlink pointing to a non-existent file.
    let _ = baml.sys.shell("ln -s " + root + "/missing.txt " + root + "/dangling.txt", null);
    let g = baml.glob.new("*.txt");
    let result = g.scan(baml.glob.ScanOptions { cwd: root, follow_symlinks: true });
    let _ = baml.sys.shell("rm -rf " + root, null);
    result
}

test "glob_scan_skips_broken_symlinks_by_default_length" {
    assert.equal(scan_skips_broken_symlinks_by_default().length(), 1)
}

test "glob_scan_skips_broken_symlinks_by_default_has_real" {
    assert.is_true(scan_skips_broken_symlinks_by_default().includes("real.txt"))
}

// ─── glob_scan_throws_on_broken_symlink_when_opted_in (unix) ─────────────────

// throw_error_on_broken_symlink=true: a dangling symlink makes the scan throw.
// Scan failures surface as a BAML-catchable `baml.errors.Io`.
function scan_throws_on_broken_symlink_when_opted_in_fn() -> bool {
    let root = glob_tmp("broken_throw");
    baml.fs.mkdir(root, baml.fs.MkdirOptions { recursive: true });
    let _ = baml.fs.write(root + "/real.txt", "content");
    // Create a dangling symlink pointing to a non-existent file.
    let _ = baml.sys.shell("ln -s " + root + "/missing.txt " + root + "/dangling.txt", null);
    let g = baml.glob.new("*.txt");
    let threw = {
        let _ = g.scan(baml.glob.ScanOptions {
            cwd: root,
            follow_symlinks: true,
            throw_error_on_broken_symlink: true,
        });
        false
    } catch (e) {
        baml.errors.Io => true
    };
    let _ = baml.sys.shell("rm -rf " + root, null);
    threw
}

test "glob_scan_throws_on_broken_symlink_when_opted_in" {
    assert.is_true(scan_throws_on_broken_symlink_when_opted_in_fn())
}

// ─── glob_scan_propagates_permission_errors (unix) ───────────────────────────

// A directory the walker can enter the parent of but not read must surface as
// an Io error, not be silently swallowed — throw_error_on_broken_symlink is
// scoped to broken symlinks only; real I/O errors propagate by default.
// Permissions are set with `chmod` via baml.sys.shell. Mode 000 → no
// permissions; root can still read, so probe first and trivially pass there
// (CI containers) — the test premise is invalid under root.
function scan_propagates_permission_errors_fn() -> bool {
    let root = glob_tmp("perm_err");
    baml.fs.mkdir(root, baml.fs.MkdirOptions { recursive: true });
    let _ = baml.fs.write(root + "/ok.txt", "ok");
    let locked = root + "/locked";
    baml.fs.mkdir(locked, baml.fs.MkdirOptions { recursive: false });
    let _ = baml.fs.write(locked + "/inner.txt", "x");
    let _ = baml.sys.shell("chmod 000 " + locked, null);
    let still_readable = baml.sys.shell("ls " + locked, null).ok();
    let threw = if (still_readable) {
        true
    } else {
        {
            let _ = baml.glob.new("**/*.txt").scan(root);
            false
        } catch (e) {
            baml.errors.Io => true
        }
    };
    // Restore permissions so the temp dir can be cleaned up.
    let _ = baml.sys.shell("chmod 755 " + locked, null);
    let _ = baml.sys.shell("rm -rf " + root, null);
    threw
}

test "glob_scan_propagates_permission_errors" {
    assert.is_true(scan_propagates_permission_errors_fn())
}
