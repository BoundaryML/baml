//! The earliest earlier file whose definitions a file needs
//! (`RecordingFile.definitions_from`, format minor 16).
//!
//! Definitions are written once per recording, in the file open when the
//! runtime first referenced them, their home. A span's end record needs its
//! thread's definition and clock epoch, and its call path with every path
//! above it, crossing into a spawner's paths through a future's spawn path,
//! each with the function it reaches: what a reader needs to name the span,
//! place it in the profile and convert its ticks. Paths and threads are
//! defined after what they reference, so each keeps the earliest home in its
//! closure, computed once when it is defined; a record then costs two
//! lookups. A file needs the earliest closure of its records.

use rustc_hash::FxHashMap;

#[derive(Default)]
pub(crate) struct Needs {
    functions: FxHashMap<u64, u64>,
    paths: FxHashMap<u32, u64>,
    threads: FxHashMap<u64, u64>,
    /// The earliest home the open file's records need.
    file: Option<u64>,
}

impl Needs {
    /// Function `id` is defined in file `home`.
    pub(crate) fn function(&mut self, id: u64, home: u64) {
        self.functions.entry(id).or_insert(home);
    }

    /// Call path `id`, under `parent` (0 at the top) and reaching `callee`,
    /// is defined in file `home`.
    pub(crate) fn path(&mut self, id: u32, parent: u32, callee: u64, home: u64) {
        let mut closure = home;
        if parent != 0
            && let Some(&above) = self.paths.get(&parent)
        {
            closure = closure.min(above);
        }
        if let Some(&function) = self.functions.get(&callee) {
            closure = closure.min(function);
        }
        self.paths.entry(id).or_insert(closure);
    }

    /// Thread `id`, with its clock epoch and spawned at `spawn_path` (0 for
    /// none), is defined in file `home`.
    pub(crate) fn thread(&mut self, id: u64, spawn_path: u32, home: u64) {
        let mut closure = home;
        if spawn_path != 0
            && let Some(&path) = self.paths.get(&spawn_path)
        {
            closure = closure.min(path);
        }
        self.threads.entry(id).or_insert(closure);
    }

    fn need(&mut self, home: Option<u64>) {
        if let Some(home) = home {
            self.file = Some(self.file.map_or(home, |file| file.min(home)));
        }
    }

    /// A span of thread `thread` at call path `path` (0 for none) is written
    /// into the open file.
    pub(crate) fn span(&mut self, thread: u64, path: u32) {
        self.need(self.threads.get(&thread).copied());
        if path != 0 {
            self.need(self.paths.get(&path).copied());
        }
    }

    /// A log of thread `thread` emitted by `function` is written into the
    /// open file.
    pub(crate) fn log(&mut self, thread: u64, function: u64) {
        self.need(self.threads.get(&thread).copied());
        self.need(self.functions.get(&function).copied());
    }

    /// The open file, `sequence`, is sealed: the earliest earlier file it
    /// needs, or 0 when it needs none.
    pub(crate) fn seal(&mut self, sequence: u64) -> u64 {
        self.file
            .take()
            .filter(|&home| home < sequence)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_needs_the_earliest_home_its_spans_reach() {
        let mut needs = Needs::default();
        needs.function(100, 1);
        needs.thread(1, 0, 1);
        needs.path(5, 0, 100, 2);
        needs.function(101, 3);
        needs.path(6, 5, 101, 3);
        // A span at path 6 reaches path 5 and function 100, and its thread.
        needs.span(1, 6);
        assert_eq!(needs.seal(4), 1);
        // Nothing written: nothing needed.
        assert_eq!(needs.seal(5), 0);
    }

    #[test]
    fn what_the_file_itself_defines_is_not_needed_from_an_earlier_one() {
        let mut needs = Needs::default();
        needs.function(100, 7);
        needs.thread(1, 0, 7);
        needs.path(5, 0, 100, 7);
        needs.span(1, 5);
        assert_eq!(needs.seal(7), 0);
    }

    #[test]
    fn a_future_needs_its_spawners_paths_through_its_spawn_path() {
        let mut needs = Needs::default();
        needs.function(100, 2);
        needs.thread(1, 0, 3);
        needs.path(5, 0, 100, 3);
        needs.path(9, 5, 100, 4);
        // Future 2, spawned at path 9, defined in file 6.
        needs.thread(2, 9, 6);
        needs.span(2, 0);
        assert_eq!(needs.seal(8), 2);
    }
}
